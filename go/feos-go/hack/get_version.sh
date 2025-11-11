#!/bin/bash

# For non-tagged commits, this is a simple "vX.X.X-XX-gXXXXXXX"
# For feos tagged commits, this is "vX.X.X"
# For feos-go tagged commits, this is "go/dpservice-go/vX.X.X-XX-gXXXXXXX"
GITVER=$(git describe --tags --always)

# The solution is to simply print the last item of an array created by splitting the Git version
ARRAY=(${GITVER//\// })
echo ${ARRAY[*]: -1}
