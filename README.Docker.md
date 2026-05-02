# Docker Guide

This repository includes a Docker image, a Compose setup, and a smoke test for
running Redlike as a containerized TCP service.

Redlike listens on port `6379` in the container. The provided
[`compose.yaml`](/home/colinc/redlike/compose.yaml) publishes that port to the
host, mounts a named Docker volume at `/data`, and sets
`ARCHIVE_PATH=/data/archive` so persisted state survives a clean container
restart.

## Quick Start

Build and start the service:

```bash
docker compose up --build
```

Redlike is then reachable on `127.0.0.1:6379`.

Because Redlike is a raw TCP service, use a Redis-compatible client, `nc`, or
another TCP tool rather than a browser.

Example `PING`:

```bash
printf '*1\r\n$4\r\nPING\r\n' | nc -N 127.0.0.1 6379
```

Expected response:

```text
+PONG
```

Stop the service:

```bash
docker compose down
```

Remove the service and its named volume:

```bash
docker compose down -v
```

## Persistence

The Compose setup uses a named volume:

```yaml
services:
  server:
    volumes:
      - data:/data

volumes:
  data:
```

That means archive data is stored in Docker-managed persistent storage, not in
a visible project directory. The archive is saved during graceful shutdown and
loaded again on startup.

The runtime image uses an entrypoint script to fix ownership on the mounted
`/data` volume before launching the server as the non-root `appuser`. That is
necessary because volume mounts replace the image-layer `/data` directory.

## Build the Image Directly

To build the image without Compose:

```bash
docker build -t redlike .
```

To run it directly:

```bash
docker run --rm -p 6379:6379 -e ADDRESS=0.0.0.0 -e PORT=6379 redlike
```

To enable archive persistence when running directly, mount writable storage and
set `ARCHIVE_PATH`, for example:

```bash
docker run --rm \
  -p 6379:6379 \
  -e ADDRESS=0.0.0.0 \
  -e PORT=6379 \
  -e ARCHIVE_PATH=/data/archive \
  -v redlike_data:/data \
  redlike
```

## Smoke Test

The repository includes a Docker smoke test script at
[`scripts/docker-smoke-test.sh`](/home/colinc/redlike/scripts/docker-smoke-test.sh).
It verifies that:

* the container starts
* `PING` returns `PONG`
* `SET` and `GET` work over RESP
* data persists across a clean `docker compose down` and restart

Run it locally with:

```bash
./scripts/docker-smoke-test.sh
```

On failure, the script prints `docker compose logs server` before cleaning up.

## CI

Docker smoke testing runs in GitHub Actions via
[`.github/workflows/docker.yml`](/home/colinc/redlike/.github/workflows/docker.yml).
That workflow is separate from the Rust workflow so container checks stay
independent from `cargo fmt`, `clippy`, and Rust test execution.

## Example AWS Deployment

The [`terraform/`](/home/colinc/redlike/terraform) directory contains an
example AWS deployment. It is intended as a reference deployment rather than a
drop-in production module. You should review the defaults, CIDR ranges, IAM
permissions, image tag, and cost profile before applying it in your own AWS
account.

The Terraform is split into two stacks:

* [`terraform/registry`](/home/colinc/redlike/terraform/registry) creates an
  ECR repository for Redlike.
* [`terraform/runtime`](/home/colinc/redlike/terraform/runtime) creates the
  runtime infrastructure: VPC, public and private subnets, a public Network
  Load Balancer, security groups, VPC endpoints, an ECS cluster, an ECS
  Fargate task definition, an ECS service, a CloudWatch log group, and EFS
  storage for archive persistence.

Both stacks are configured for Terraform Cloud workspaces. If you use a
different backend, update the `terraform` blocks in each stack before running
`terraform init`.

The runtime stack runs the service in private subnets. It does not use a NAT
gateway. Instead, it creates interface endpoints for ECR API, ECR Docker, and
CloudWatch Logs, plus an S3 gateway endpoint so Fargate can pull ECR image
layers and write logs without public outbound internet access.

The public entrypoint is the Network Load Balancer DNS name exposed as the
`app_endpoint` output. The NLB security group allows client traffic only from
the CIDR blocks supplied through `allowed_client_cidr_blocks`.

The ECS task mounts EFS at `/data` and sets `ARCHIVE_PATH=/data/archive`, which
matches the Compose persistence layout. When ECS stops a task during scale-down
or deployment replacement, Redlike handles the shutdown signal and saves the
archive to EFS. The next task loads the archive from the same path.

### Build and Push an Image

The runtime task definition uses the `container_image` variable. Point it at an
image that already exists in ECR, for example:

```hcl
container_image = "848973819276.dkr.ecr.us-west-2.amazonaws.com/redlike:main"
```

For repeatable deployments, prefer immutable tags such as Git SHAs:

```bash
GIT_SHA=$(git rev-parse --short HEAD)
IMAGE="848973819276.dkr.ecr.us-west-2.amazonaws.com/redlike:git-$GIT_SHA"
```

The Dockerfile uses BuildKit features. If your Docker installation has the
BuildKit plugin available, build and push with:

```bash
aws ecr get-login-password --region us-west-2 \
  | docker login --username AWS --password-stdin 848973819276.dkr.ecr.us-west-2.amazonaws.com

DOCKER_BUILDKIT=1 docker build -t "$IMAGE" .
docker push "$IMAGE"
```

You can also use an existing ECR tag or digest and set `container_image` to
that exact value.

### Runtime Inputs

Common values to change in [`terraform/runtime/variables.tf`](/home/colinc/redlike/terraform/runtime/variables.tf):

* `aws_region`: AWS region for the runtime stack.
* `container_image`: ECR image URI used by the ECS task definition.
* `allowed_client_cidr_blocks`: named CIDR allowlist for clients that can
  connect to the public NLB on the Redlike port.
* `app_port`: TCP port exposed by Redlike. The default is `6379`.
* `task_cpu` and `task_memory`: Fargate task size. The example defaults to
  `256` CPU units and `512` MiB.
* `desired_count`: number of Redlike ECS tasks to run. Set it to `0` to stop
  running Fargate tasks while leaving the surrounding infrastructure in place.
* `efs_name` and `efs_sg_name`: names for the EFS file system and security
  group used for archive persistence.
* `cluster_name`, `service_name`, `nlb_name`, and security group names:
  resource names used in AWS.
* VPC, subnet CIDR blocks, and availability zones: network layout for the
  example VPC.

Keep local values such as personal IP allowlists out of Git. One option is an
ignored local file:

```hcl
# terraform/runtime/local.auto.tfvars
allowed_client_cidr_blocks = {
  home = "203.0.113.10/32"
}

container_image = "848973819276.dkr.ecr.us-west-2.amazonaws.com/redlike:main"
```

### Apply Order

Create the ECR repository first:

```bash
cd terraform/registry
terraform init
terraform apply
```

Then build and push the container image, and apply the runtime stack:

```bash
cd terraform/runtime
terraform init
terraform apply
```

After the runtime apply completes, test the NLB endpoint with a RESP `PING`:

```bash
printf '*1\r\n$4\r\nPING\r\n' | nc -N "$(terraform output -raw app_endpoint)" 6379
```

Expected response:

```text
+PONG
```

To verify persistence, write a key, scale the service to zero, then scale it
back to one and read the key again. The stopped task's CloudWatch log stream
should include messages like:

```text
saving archive to path p=/data/archive
archiving successful p=/data/archive
```

The replacement task should log that it loaded the archive on startup.

### Cost and Operations Notes

The example uses ECS Fargate, a public Network Load Balancer, three interface
VPC endpoints, and EFS. Those resources can incur charges while the stack
exists. Scaling the ECS service to zero stops Fargate task CPU and memory
charges, but the load balancer, interface endpoints, and EFS can still cost
money.

The example runs one ECS task by default. To reduce Fargate task charges while
leaving the surrounding infrastructure in place, apply with `desired_count = 0`.
Set it back to `1` to start the service again.

The runtime stack manages an `ecsTaskExecutionRole` with the AWS-managed
`AmazonECSTaskExecutionRolePolicy`. If you already created that role outside
Terraform, import it into the runtime workspace before applying so Terraform
does not try to create a duplicate role.
