# This makefile assumes you have cargo and the x86_64-unknown-linux-musl target installed.
CARGO ?= cargo

tinyfs-rs: src/*.rs Cargo.toml
	$(CARGO) build --release --target x86_64-unknown-linux-musl

pack: tinyfs-rs
	cp target/x86_64-unknown-linux-musl/release/tinyfs-rs tinyfs-rs
	strip tinyfs-rs
	upx tinyfs-rs
	tar -czvf tinyfs-rs.tgz Cargo.* src Makefile README* tinyfs-rs

.PHONY: clean
clean:
	$(CARGO) clean
	rm -f tinyfs-rs
