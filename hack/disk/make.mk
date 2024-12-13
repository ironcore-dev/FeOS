disk: hack/disk/bootloader_util hack/disk/mbr.bin
	head -c 64M /dev/zero > target/disk
	printf 'label: gpt\nstart=2048, type=uefi, name=EFI\n' | /usr/sbin/sfdisk target/disk
	mformat -i target/disk@@2048S -F -v EFI ::
	mmd -i target/disk@@2048S ::/EFI
	mmd -i target/disk@@2048S ::/EFI/BOOT
	mcopy -i target/disk@@2048S target/uki.efi ::/EFI/BOOT/BOOTX64.EFI
	dd if=hack/disk/mbr.bin of=target/disk bs=446 count=1 conv=notrunc 2> /dev/null
	dd if=hack/disk/mbr.bin of=target/disk bs=512 iseek=1 count=2 seek=34 conv=notrunc 2> /dev/null
	./hack/disk/bootloader_util target/disk < hack/disk/boot_config.json

hack/disk/bootloader_util:
	curl -sSLf https://github.com/nkraetzschmar/bootloader/releases/download/v0.0.1/bootloader_util > '$@'
	chmod +x '$@'

hack/disk/mbr.bin:
	curl -sSLf https://github.com/nkraetzschmar/bootloader/releases/download/v0.0.1/mbr.bin > '$@'
