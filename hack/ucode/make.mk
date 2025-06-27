ucode:
	docker run --rm -u $${UID} -v "`pwd`:/feos" feos-builder ./hack/ucode/mk-ucode
