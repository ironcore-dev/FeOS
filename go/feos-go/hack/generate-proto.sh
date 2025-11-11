#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
# SPDX-License-Identifier: Apache-2.0

set -e

BASEDIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

GO_PROJECT_ROOT="$BASEDIR/.."

FEOS_ROOT="$GO_PROJECT_ROOT/../.."

"$BASEDIR/get_version.sh" > "$GO_PROJECT_ROOT/generated_from.txt"

echo "Generating FeOS protobuf Go files..."

PROTO_PATH_ROOT="$FEOS_ROOT/proto"

PROTO_FILES_V1="$FEOS_ROOT/proto/v1/*.proto"

GO_MODULE_NAME="github.com/FeOS/go/feos-go"

OUTPUT_DIR="$GO_PROJECT_ROOT"

mkdir -p "$OUTPUT_DIR"

protoc --experimental_allow_proto3_optional \
    --proto_path="$PROTO_PATH_ROOT" \
    --go_out="$OUTPUT_DIR" \
    --go_opt=module="$GO_MODULE_NAME" \
    --go-grpc_out="$OUTPUT_DIR" \
    --go-grpc_opt=module="$GO_MODULE_NAME" \
    $PROTO_FILES_V1

echo "Protobuf generation complete."

BOILERPLATE_FILE="$BASEDIR/boilerplate.go.txt"

if [ ! -f "$BOILERPLATE_FILE" ]; then
    echo "Warning: boilerplate.go.txt not found, skipping license header."
else
    for file in $(find "$OUTPUT_DIR" -name '*.pb.go'); do
      boilerplate="$(cat "$BOILERPLATE_FILE")"
      echo -e "$boilerplate\n$(cat "$file")" > "$file"
    done
    echo "Applied license boilerplate."
fi
