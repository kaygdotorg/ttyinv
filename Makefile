PYTHON ?= python3
VENV ?= .venv
PY := $(VENV)/bin/python
PIP := $(VENV)/bin/pip

.PHONY: install test lint privacy schema visual build check clean

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

visual:
	$(PY) -m playwright install chromium
	$(PY) scripts/vendor_geist_mono.py
	rm -rf artifacts && mkdir -p artifacts
	$(PY) -m ttyinv render examples/reference.md --format both --theme dark --deterministic --output artifacts/reference
	$(PY) scripts/visual_contract.py artifacts/reference.html --screenshot artifacts/reference.png --report artifacts/visual-report.json

build:
	rm -rf build dist
	$(PY) -m build

check: test lint privacy schema

clean:
	rm -rf $(VENV) build dist artifacts .pytest_cache .coverage htmlcov
	find . -type d -name __pycache__ -prune -exec rm -rf {} +
