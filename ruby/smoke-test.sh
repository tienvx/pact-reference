#!/bin/bash

set -e
set -x

case $1 in
x86_64-unknown-linux-musl)
    docker run --platform=linux/amd64 --rm -v $(pwd)/..:/home -w /home/ruby ruby:alpine ruby test_ffi.rb
    ;;
aarch64-unknown-linux-musl)
    docker run --platform=linux/arm64 --rm -v $(pwd)/..:/home -w /home/ruby ruby:alpine ruby test_ffi.rb
    ;;
x86_64-unknown-linux-gnu)
    docker run --platform=linux/amd64 --rm -v $(pwd)/..:/home -w /home/ruby ruby:slim ruby test_ffi.rb
    ;;
aarch64-unknown-linux-gnu)
    docker run --platform=linux/arm64 --rm -v $(pwd)/..:/home -w /home/ruby ruby:slim ruby test_ffi.rb
    ;;
aarch64-pc-windows-msvc)
    echo unable to test in github actions
    exit 0
    ;;
*) ruby test_ffi.rb ;;
esac
