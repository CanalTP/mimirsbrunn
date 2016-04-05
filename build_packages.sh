#! /bin/bash

mimirsbrunn_dir="`dirname \"$0\"`"
temporary_install_dir="./build_packages/"
raw_version="`git describe`"

# for debian version number, we don't want the leading 'v' of the git tag
version=`echo $raw_version | sed -e 's#v\(.*\)#\1#'`

mkdir -p $temporary_install_dir

# build and install mimirsbrunn to a temporary directory
cargo install --path=$mimirsbrunn_dir --root=$temporary_install_dir


# create debian packages
fpm -s dir -t deb \
    --name mimirsbrunn \
    --version $version \
    --force \
    --exclude *.crates.toml \
    $temporary_install_dir=/usr/

