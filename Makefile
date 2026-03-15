.PHONY: test-integration test release-gate

test-integration:
	@echo "Installing python dependencies..."
	@pip3 install msgpack >/dev/null 2>&1 || true
	@echo "Running cross-language integration tests..."
	@cd rust && cargo test --test cross_language_integration -p bridge_rx

test: test-integration
	@echo "Running all unit tests..."
	@cd rust && cargo test

release-gate:
	@set -e; \
	echo "\n============================================="; \
	echo "  Running Release Gate Validation Pipeline   "; \
	echo "=============================================\n"; \
	echo "Step 1: cargo fmt --check..."; \
	(cd rust && cargo fmt --check) || { echo "❌ FATAL: Step 1 failed: Formatting violations detected. Run 'cargo fmt'."; exit 1; }; \
	echo "✅ Step 1 passed.\n"; \
	\
	echo "Step 2: cargo clippy -D warnings..."; \
	(cd rust && cargo clippy --workspace -- -D warnings) || { echo "❌ FATAL: Step 2 failed: Clippy warnings detected. Fix them before release."; exit 1; }; \
	echo "✅ Step 2 passed.\n"; \
	\
	echo "Step 3: cargo test --workspace..."; \
	(cd rust && cargo test --workspace) || { echo "❌ FATAL: Step 3 failed: Rust workspace tests failed."; exit 1; }; \
	echo "✅ Step 3 passed.\n"; \
	\
	echo "Step 4: python -m pytest python/tests/..."; \
	pip3 install pytest msgpack >/dev/null 2>&1 || true; \
	(cd python && python3 -m pytest tests/) || { echo "❌ FATAL: Step 4 failed: Python tests failed."; exit 1; }; \
	echo "✅ Step 4 passed.\n"; \
	\
	echo "Step 5: make test-integration..."; \
	$(MAKE) test-integration || { echo "❌ FATAL: Step 5 failed: Cross-language integration tests failed."; exit 1; }; \
	echo "✅ Step 5 passed.\n"; \
	\
	echo "Step 6: replay smoke test..."; \
	(cd rust && cargo run -p replayer -- bins/replayer/tests/sample_data.csv /tmp/smoke_test_golden.json ../configs/default.toml > /dev/null 2>&1) || { echo "❌ FATAL: Step 6 failed: Replayer smoke test crashed."; exit 1; }; \
	echo "✅ Step 6 passed.\n"; \
	\
	echo "🎉 SUCCESS: Release Gate passed completely! System is ready for production.";
