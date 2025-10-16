YOUKI_FILENAME = youki

target/youki:
	mkdir -p target/youki/target
	curl -L "https://github.com/youki-dev/youki/releases/download/v$(shell cat hack/youki/version)/$(YOUKI_FILENAME)-$(shell cat hack/youki/version)-x86_64-musl.tar.gz" -o "target/youki/target/$(YOUKI_FILENAME).tar.gz"
	tar -xzf target/youki/target/$(YOUKI_FILENAME).tar.gz -C target/youki/target
	rm target/youki/target/$(YOUKI_FILENAME).tar.gz
	rm target/youki/target/LICENSE
	rm target/youki/target/README.md
	chmod a+x target/youki/target/$(YOUKI_FILENAME)