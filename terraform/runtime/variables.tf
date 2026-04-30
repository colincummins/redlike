variable "aws_region" {
  description = "AWS region for runtime resources"
  type        = string
  default     = "us-west-2"
}

variable "cluster_name" {
  description = "Name of the ECS cluster"
  type        = string
  default     = "redlike-cluster"
}

variable "vpc_cidr_block" {
  description = "CIDR block for the runtime VPC"
  type        = string
  default     = "10.0.0.0/16"
}

variable "public1_cidr_block" {
  description = "CIDR block for public subnet 1"
  type        = string
  default     = "10.0.0.0/24"
}

variable "public2_cidr_block" {
  description = "CIDR block for public subnet 2"
  type        = string
  default     = "10.0.1.0/24"
}

variable "public1_availability_zone" {
  description = "Availability zone for public subnet 1"
  type        = string
  default     = "us-west-2a"
}

variable "public2_availability_zone" {
  description = "Availability zone for public subnet 2"
  type        = string
  default     = "us-west-2b"
}


variable "private1_cidr_block" {
  description = "CIDR block for private subnet 1"
  type        = string
  default     = "10.0.2.0/24"
}

variable "private2_cidr_block" {
  description = "CIDR block for private subnet 2"
  type        = string
  default     = "10.0.3.0/24"
}

variable "private1_availability_zone" {
  description = "Availability zone for private subnet 1"
  type        = string
  default     = "us-west-2a"
}

variable "private2_availability_zone" {
  description = "Availability zone for private subnet 2"
  type        = string
  default     = "us-west-2b"
}

variable "nlb_name" {
  description = "Name of the Network Load Balancer"
  type        = string
  default     = "redlike-lb"
}

variable "nlb_target_group_name" {
  description = "Name of the Network Load Balancer target group"
  type        = string
  default     = "redlike-lb-target-group"
}

variable "app_port" {
  description = "TCP port exposed by the Redlike service"
  type        = number
  default     = 6379
}

variable "nlb_sg_name" {
  description = "Name of the Network Load Balancer security group"
  type        = string
  default     = "redlike-nlb-sg"
}

variable "allowed_client_cidr_blocks" {
  description = "Named CIDR blocks allowed to connect to the public NLB"
  type        = map(string)
  sensitive   = true
}

variable "app_sg_name" {
  description = "Name of the security group for private app instances"
  type        = string
  default     = "redlike-app-sg"
}

variable "cloudwatch_log_group_name" {
  description = "Name of the ecs cloudwatch log group"
  type        = string
  default     = "ecs/redlike"
}

variable "container_image" {
  description = "ECS Container Image"
  type        = string
  default     = "848973819276.dkr.ecr.us-west-2.amazonaws.com/redlike:main"
}

variable "task_cpu" {
  description = "ECS CPU"
  type        = number
  default     = 256
}

variable "task_memory" {
  description = "ECS Task Memory"
  type        = number
  default     = 512
}

variable "service_name" {
  description = "ECS Service Name"
  type        = string
  default     = "redlike-service"
}

variable "desired_count" {
  description = "Number of Redlike ECS tasks to run"
  type        = number
  default     = 1
}
