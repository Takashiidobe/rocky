SOURCE := presentation.tex
BUILD_DIR := pdf_build
OUTPUT := export_presentation.pdf
DEPENDENCIES := $(SOURCE) wmtheme.sty $(wildcard images/*)

.PHONY: all pdf clean

all: pdf

pdf: $(OUTPUT)

$(OUTPUT): $(DEPENDENCIES)
	mkdir -p $(BUILD_DIR)
	latexmk -pdf -interaction=nonstopmode -halt-on-error -outdir=$(BUILD_DIR) $(SOURCE)
	cp $(BUILD_DIR)/presentation.pdf $(OUTPUT)

clean:
	latexmk -C -outdir=$(BUILD_DIR) $(SOURCE)
	rm -f $(OUTPUT)
