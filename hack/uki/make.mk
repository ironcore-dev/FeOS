keys:
	mkdir keys
	chmod 700 keys
	cp hack/uki/secureboot-cert.conf keys/
	openssl genrsa -out keys/secureboot.key 2048
	openssl req -config keys/secureboot-cert.conf -new -x509 -newkey rsa:2048 -keyout keys/secureboot.key -outform PEM -out keys/secureboot.pem -nodes -days 3650 -subj "/CN=FeOS/"
	openssl x509 -in keys/secureboot.pem -out keys/secureboot.der -outform DER

uki: keys
	docker run --rm -u $${UID} -v "`pwd`:/feos" feos-builder ukify build \
	  --os-release @/feos/hack/uki/os-release.txt \
	  --linux /feos/target/kernel/vmlinuz \
	  --initrd /feos/target/initramfs.zst \
	  --cmdline @/feos/target/cmdline \
	  --secureboot-private-key /feos/keys/secureboot.key \
	  --secureboot-certificate /feos/keys/secureboot.pem \
	  --output /feos/target/uki.efi
