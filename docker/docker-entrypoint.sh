#!/bin/sh
set -e
mkdir -p /data 
echo "created data dir"
chown appuser:appuser /data 
echo "dir now belongs to appuser"
exec su-exec appuser:appuser /bin/server