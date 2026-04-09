resource "aws_ecr_repository" "redlike" {
  name = var.repository_name
}

resource "aws_ecr_lifecycle_policy" "redlike" {
  repository = aws_ecr_repository.redlike.name

  policy = jsonencode({
    rules = [
      {
        rulePriority = 1
        description  = "Expire untagged images after 7 days"
        selection = {
          tagStatus   = "untagged"
          countType   = "sinceImagePushed"
          countUnit   = "days"
          countNumber = 7
        }
        action = {
          type = "expire"
        }
      }
    ]
  })
}
