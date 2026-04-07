#!/usr/bin/env bash
docker build -t redlike-smoke-test ../ &&\
docker compose up --detach &&\
docker compose down