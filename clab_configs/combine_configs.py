from pathlib import Path

ROOT_DIR = Path("startup_configs")  # Set your target root directory here
OUTPUT_FILE = Path("combined_startup.cli")     # Output destination file

def combine_text_files(root: Path, output_path: Path):
    root = root.resolve()
    output_path = output_path.resolve()
    
    with output_path.open("w", encoding="utf-8") as outfile:
        for file_path in sorted(root.rglob("*")):
            # Skip directories and prevent reading the output file if saved inside ROOT_DIR
            if file_path.is_file() and file_path != output_path:
                try:
                    rel_path = file_path.relative_to(root)
                    content = file_path.read_text(encoding="utf-8", errors="replace")
                    
                    outfile.write(f"[{rel_path}]\n\n")
                    outfile.write(content.rstrip("\n"))
                    outfile.write("\n\n")
                except Exception as e:
                    print(f"Skipping {file_path}: {e}")

if __name__ == "__main__":
    combine_text_files(ROOT_DIR, OUTPUT_FILE)