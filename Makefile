.PHONY: fonts check-release-fonts release test privacy check example clean

fonts:
	python scripts/vendor_geist_mono.py

check-release-fonts:
	python scripts/check_release_fonts.py

release: fonts check-release-fonts
	python -m build

privacy:
	python scripts/privacy_check.py

test:
	PYTHONPATH=src pytest

check: privacy test

example:
	PYTHONPATH=src python -m ttyinv examples/simple.md --format both

clean:
	rm -rf build dist .pytest_cache **/__pycache__ examples/*.html examples/*.pdf
