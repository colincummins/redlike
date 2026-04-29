resource "aws_vpc" "main" {
  cidr_block = var.vpc_cidr_block

  tags = {
    Name = "redlike"
  }
}

resource "aws_subnet" "public1" {
  vpc_id            = aws_vpc.main.id
  cidr_block        = var.public1_cidr_block
  availability_zone = var.public1_availability_zone
  tags = {
    Name = "redlike-public-1"
  }
}

resource "aws_subnet" "public2" {
  vpc_id            = aws_vpc.main.id
  cidr_block        = var.public2_cidr_block
  availability_zone = var.public2_availability_zone
  tags = {
    Name = "redlike-public-2"
  }
}

resource "aws_subnet" "private1" {
  vpc_id            = aws_vpc.main.id
  cidr_block        = var.private1_cidr_block
  availability_zone = var.private1_availability_zone

  tags = {
    Name = "redlike-private-1"
  }
}

resource "aws_subnet" "private2" {
  vpc_id            = aws_vpc.main.id
  cidr_block        = var.private2_cidr_block
  availability_zone = var.private2_availability_zone

  tags = {
    Name = "redlike-private-2"
  }
}

resource "aws_internet_gateway" "main" {
  vpc_id = aws_vpc.main.id

  tags = {
    Name = "redlike"
  }
}

resource "aws_route_table" "public" {
  vpc_id = aws_vpc.main.id
  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.main.id
  }

  tags = {
    Name = "public"
  }
}

resource "aws_route_table_association" "public1" {
  subnet_id      = aws_subnet.public1.id
  route_table_id = aws_route_table.public.id
}

resource "aws_route_table_association" "public2" {
  subnet_id      = aws_subnet.public2.id
  route_table_id = aws_route_table.public.id
}

resource "aws_lb" "main" {
  name               = var.nlb_name
  subnets            = [aws_subnet.public1.id, aws_subnet.public2.id]
  internal           = false
  load_balancer_type = "network"
  security_groups    = [aws_security_group.nlb.id]

  tags = {
    Name = var.nlb_name
  }
}

resource "aws_lb_target_group" "main" {
  name        = var.nlb_target_group_name
  protocol    = "TCP"
  port        = var.app_port
  vpc_id      = aws_vpc.main.id
  target_type = "ip"

  health_check {
    protocol = "TCP"
    port     = "traffic-port"
  }

  tags = {
    Name = var.nlb_target_group_name
  }
}

resource "aws_lb_listener" "main" {
  load_balancer_arn = aws_lb.main.arn
  protocol          = "TCP"
  port              = var.app_port

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.main.arn
  }
}

resource "aws_security_group" "nlb" {
  name        = var.nlb_sg_name
  description = "Allow whitelisted redis port tcp traffic through"
  vpc_id      = aws_vpc.main.id

  tags = {
    Name = var.nlb_sg_name
  }
}

resource "aws_vpc_security_group_ingress_rule" "nlb_app_port" {
  for_each = nonsensitive(toset(keys(var.allowed_client_cidr_blocks)))

  security_group_id = aws_security_group.nlb.id
  cidr_ipv4         = var.allowed_client_cidr_blocks[each.key]
  ip_protocol       = "tcp"
  from_port         = var.app_port
  to_port           = var.app_port
}

resource "aws_vpc_security_group_egress_rule" "nlb_app_port" {
  security_group_id = aws_security_group.nlb.id
  cidr_ipv4         = var.vpc_cidr_block
  ip_protocol       = "tcp"
  from_port         = var.app_port
  to_port           = var.app_port
}

resource "aws_security_group" "app" {
  name        = var.app_sg_name
  description = "Allow only traffic from the NLB"
  vpc_id      = aws_vpc.main.id

  tags = {
    Name = var.app_sg_name
  }
}

resource "aws_vpc_security_group_ingress_rule" "app_from_nlb" {
  security_group_id            = aws_security_group.app.id
  referenced_security_group_id = aws_security_group.nlb.id
  ip_protocol                  = "tcp"
  from_port                    = var.app_port
  to_port                      = var.app_port
}

resource "aws_vpc_security_group_egress_rule" "app_all" {
  security_group_id = aws_security_group.app.id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
}

resource "aws_ecs_cluster" "main" {
  name = var.cluster_name

  tags = {
    Name = var.cluster_name
  }
}

resource "aws_iam_role" "ecs_task_execution" {
  name = "ecsTaskExecutionRole"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Principal = {
          Service = "ecs-tasks.amazonaws.com"
        }
        Action = "sts:AssumeRole"
      }
    ]
  })

  tags = {
    Name = "ecsTaskExecutionRole"
  }
}

resource "aws_iam_role_policy_attachment" "ecs_task_execution" {
  role       = aws_iam_role.ecs_task_execution.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

resource "aws_cloudwatch_log_group" "app" {
  name              = var.cloudwatch_log_group_name
  retention_in_days = 14
  tags = {
    Name = var.cloudwatch_log_group_name
  }
}
