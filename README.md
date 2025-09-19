# FeOS

[![REUSE status](https://api.reuse.software/badge/github.com/ironcore-dev/dpservice)](https://api.reuse.software/info/github.com/ironcore-dev/FeOS)
[![GitHub License](https://img.shields.io/static/v1?label=License&message=Apache-2.0&color=blue)](LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://makeapullrequest.com)

## Overview
FeOS is a revolutionary init system for Linux, designed specifically for hypervisors and servers that run containers. Unlike traditional systems that use sysvinit or systemd, FeOS boots directly from the Linux Kernel. Written in Rust for enhanced security and memory safety, FeOS is an ideal solution for multi-tenant environments, offering robust protection against common vulnerabilities like buffer overflows.

### Use Cases
**Multi-Tenant Environments**: Ideal for environments requiring strong isolation between different tenants' workloads.  
**Clustered Server Environments**: Optimized for clustered server environments, providing efficient management and execution of containers and VMs.

### Key Features
**Init System**: FeOS replaces traditional Linux init systems, booting directly from the Linux Kernel.  
**Container and VM Support**: Supports running VMs, Containers, and MicroVMs, including the ability to run containers within MicroVMs for enhanced security.  
**Rust-Based**: Developed in Rust, leveraging memory-safe programming to mitigate common vulnerabilities.  
**Compatibility**: Compatible with Rust-based container and VM runtimes like youki, rustvmm, and cloud-hypervisor.  
**Cluster Integration**: Designed for use in clustered environments, FeOS treats servers as hardware resources for executing programs.  
**gRPC API**: Exposes a gRPC API for external orchestrators to manage VMs, Containers, and MicroVMs.

## For Developers
For detailed information on contributing to FeOS, including development guidelines, setup instructions, and coding standards, please refer to our [Developer Documentation](docs/development.md). This document provides all the necessary information for developers to understand the codebase, contribute effectively, and collaborate with the FeOS community.

## Installation and Setup
Just download the Unified Kernel Image (UKI) from the [Releases](https://github.com/maltej/feos/releases) page and boot it with UEFI. You can do that in a VM by providing the UKI as a Kernel, via PXE boot or [boot from USB stick](docs/boot-image/uki.md#boot-from-usb-stick).

## Licensing
FeOS is licensed under the [Apache License v2.0](LICENSE), ensuring open-source availability and collaborative development potential.

Copyright 2023 by Malte Janduda.
