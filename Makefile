PYTHON ?= python3
VENV ?= .venv
PY := $(VENV)/bin/python
PIP := $(VENV)/bin/pip

CONTAINER ?= podman
GITLEAKS_IMAGE ?= docker.io/zricethezav/gitleaks@sha256:5d0147dc25c78f8cc2b9861ff8f5c9b4a41419ed60a9ce2217de5a215270b42b

# One authoritative case list for the visual contract.
# Fields: name:source:density:expected pages:stress flag
VISUAL_CASES := \
	normal:examples/reference.md:comfortable:1: \
	compact:examples/reference.md:compact:1: \
	multi-section:examples/gallery/expenses.md:comfortable:2: \
	long-row:examples/gallery/long-row.md:comfortable:1: \
	multi-page:examples/gallery/multi-page.md:comfortable:3:stress

.PHONY: install test lint privacy schema secrets visual build check preflight clean

install:
	$(PYTHON) -m venv $(VENV)
	$(PIP) install --upgrade pip
	$(PIP) install -c constraints-release.txt -e '.[dev]'

# Unit tests do not require a browser.
test:
	$(PY) -m pytest --cov=ttyinv --cov-report=term-missing

lint:
	@find examples -name '*.md' -exec $(PY) -m ttyinv lint {} \;

privacy:
	$(PY) scripts/privacy_check.py .

schema:
	@temporary="$$(mktemp)"; trap 'rm -f "$$temporary"' EXIT; \
		$(PY) -m ttyinv schema --output "$$temporary" >/dev/null; \
		diff -u schema/ttyinv-v1.schema.json "$$temporary"

# Scan the whole git history for secrets. Set CONTAINER=docker if you use Docker.
# label=disable lets the container read the mount on an SELinux host without
# relabelling any file on the host.
secrets:
	$(CONTAINER) run --rm --security-opt label=disable \
		--volume "$(CURDIR):/repo:ro" $(GITLEAKS_IMAGE) \
		git /repo --config /repo/.gitleaks.toml --redact --verbose

# Render every case in both themes, then assert HTML and PDF geometry.
visual:
	$(PY) -m playwright install chromium
	$(PY) scripts/vendor_geist_mono.py
	rm -rf artifacts && mkdir -p artifacts
	@status=0; \
	for case in $(VISUAL_CASES); do \
		name="$${case%%:*}"; rest="$${case#*:}"; \
		source="$${rest%%:*}"; rest="$${rest#*:}"; \
		density="$${rest%%:*}"; rest="$${rest#*:}"; \
		pages="$${rest%%:*}"; stress="$${rest#*:}"; \
		for theme in light dark; do \
			$(PY) -m ttyinv render "$$source" --format both --theme "$$theme" \
				--density "$$density" --deterministic --output "artifacts/$$name-$$theme" || status=1; \
			$(PY) scripts/visual_contract.py "artifacts/$$name-$$theme.html" \
				--screenshot "artifacts/$$name-$$theme.png" \
				--report "artifacts/$$name-$$theme-html.json" || status=1; \
		done; \
		stress_args=""; \
		if test "$$stress" = stress; then stress_args="--stress"; fi; \
		$(PY) scripts/pdf_visual_contract.py \
			"artifacts/$$name-light.pdf" "artifacts/$$name-dark.pdf" \
			--case "$$name" --expected-pages "$$pages" \
			--baseline tests/fixtures/visual_geometry.json \
			--report "artifacts/$$name-pdf.json" $$stress_args || status=1; \
	done; \
	for font in src/ttyinv/fonts/GeistMono-*.woff2; do \
		test -e "$$font" || continue; \
		$(PY) scripts/font_metrics_check.py "$$font" --json \
			--baseline tests/fixtures/font_metrics.json || status=1; \
	done; \
	exit $$status

build:
	rm -rf build dist
	$(PY) -m build

check: test lint privacy schema

# Run every gate before you push.
preflight: check secrets visual

clean:
	rm -rf $(VENV) build dist artifacts .pytest_cache .coverage htmlcov
	find . -type d -name __pycache__ -prune -exec rm -rf {} +
