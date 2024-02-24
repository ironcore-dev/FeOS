build-container:
	docker rmi feos-builder
	docker pull rust:1-bookworm
	docker build hack/build-container -t feos-builder
	mkdir -p target
	touch hack/build-container
