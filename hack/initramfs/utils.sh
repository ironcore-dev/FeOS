#!/bin/bash

copy_lib() {
    cp -Lr $1 `echo $1 | awk '{print substr($1,2); }'`
}

install_libs() {
    echo "  installing libs for $1"
    
    set +e
    libs=$(ldd $1 \
        | grep so \
        | sed -e '/^[^\t]/ d' \
        | sed -e 's/\t//' \
        | sed -e 's/.*=..//' \
        | sed -e 's/ (0.*)//' \
        | sort \
        | uniq -c \
        | sort -n \
        | awk '{$1=$1;print}' \
        | cut -d' ' -f2 \
        | grep "^/")

    set -e
    [[ $? -ne 0 ]] && return
    
    for l in ${libs[@]}; do
        copy_lib $l
    done
}
