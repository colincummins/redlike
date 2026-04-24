terraform {
  required_version = "~> 1.14.8"

  cloud {
    organization = "ColinCumminsCS"

    workspaces {
      name = "redlike-registry"
    }
  }

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
  }
}
