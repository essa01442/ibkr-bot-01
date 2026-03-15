.PHONY: test-integration test

test-integration:
	@echo "Installing python dependencies..."
	@pip3 install msgpack >/dev/null 2>&1 || true
	@echo "Running cross-language integration tests..."
	@cd rust && cargo test --test cross_language_integration -p bridge_rx

test: test-integration
	@echo "Running all unit tests..."
	@cd rust && cargo test
