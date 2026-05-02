output "app_endpoint" {
  description = "DNS name clients can use to connect to the Redlike service"
  value       = aws_lb.main.dns_name
}
