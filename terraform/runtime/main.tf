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
    Name = "public1"
  }
}

resource "aws_subnet" "public2" {
  vpc_id            = aws_vpc.main.id
  cidr_block        = var.public2_cidr_block
  availability_zone = var.public2_availability_zone
  tags = {
    Name = "public2"
  }
}

resource "aws_subnet" "private1" {
  vpc_id            = aws_vpc.main.id
  cidr_block        = var.private1_cidr_block
  availability_zone = var.private1_availability_zone

  tags = {
    Name = "private1"
  }
}

resource "aws_subnet" "private2" {
  vpc_id            = aws_vpc.main.id
  cidr_block        = var.private2_cidr_block
  availability_zone = var.private2_availability_zone

  tags = {
    Name = "private2"
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
