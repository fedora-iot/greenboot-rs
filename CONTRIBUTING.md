#### Test Coverage Requirements

This project enforces test coverage thresholds to maintain code quality:

- **Project coverage:** 75% minimum (entire codebase)
- **Patch coverage:** 75% minimum (new/changed code in PRs)

`src/main.rs` is excluded from coverage calculations (see `codecov.yml`).

Coverage is enforced through:
1. **cargo-llvm-cov** - Generates the coverage report (`tests/coverage/lcov.info`) as part of the `build_and_test_with_coverage` CI job
2. **Codecov** - Reports coverage trends and fails CI if the project/patch thresholds defined in `codecov.yml` are not met


**Before submitting a PR:**

```bash
# Run tests with coverage (installs cargo-llvm-cov automatically if missing;
# requires llvm-cov/llvm-profdata, e.g. `sudo dnf install llvm`, and uses sudo
# internally for some steps)
make test-coverage

# View the HTML report
xdg-open tests/coverage/html/index.html
```

PRs that drop coverage below these thresholds will fail CI and cannot be merged. Add tests to ensure your changes meet the coverage requirements.

If your PR would drop the coverage by a meaningful amount, even if it is still within the threshold, you may be asked to add extra tests.