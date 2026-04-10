output "repository_name" {
  value = aws_ecr_repository.redlike.name
}

output "repository_url" {
  value = aws_ecr_repository.redlike.repository_url
}

output "repository_arn" {
  value = aws_ecr_repository.redlike.arn
}
