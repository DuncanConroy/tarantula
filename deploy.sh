#!/bin/bash
while kill -9 $(cat process.pid) 2> /dev/null; do
    sleep 1
done
cp tarantula tarantula-live
screen -d -m bash -c "./tarantula-live"