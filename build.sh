#!/bin/bash

docker build --no-cache -t local/mimirsbrunn:stretch -f packages/debian/Dockerfile packages/debian
docker create -ti --name dummy local/mimirsbrunn:stretch bash
docker cp dummy:/srv/mimirsbrunn.deb mimirsbrunn.deb
docker rm -f dummy
