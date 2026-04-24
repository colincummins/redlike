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
