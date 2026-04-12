import re
import os

# Start marker: optional #/ // + optional spaces + (<<name>>= or <[name]>=)
start_pattern = re.compile(r'^\s*(#|\/\/)?\s*(<<[^>]+>>=|<\[[^\]]+\]>=)')
# End marker: optional #/ // + optional spaces + (@ or @@)
end_pattern = re.compile(r'^\s*(#|\/\/)?\s*(@|@@)\s*$')

results = {}

def process_file(file_path):
    with open(file_path, 'r', errors='ignore') as f:
        lines = f.readlines()

    chunk_name = None
    start_line = 0
    
    for i, line in enumerate(lines):
        if chunk_name is None:
            match = start_pattern.match(line)
            if match:
                chunk_name = match.group(2)
                start_line = i + 1
        else:
            if end_pattern.match(line):
                end_line = i + 1
                length = end_line - start_line - 1
                if length > 40:
                    if file_path not in results:
                        results[file_path] = []
                    results[file_path].append((chunk_name, length))
                chunk_name = None

# Gather all .adoc files
for root, dirs, files in os.walk('.'):
    for file in files:
        if file.endswith('.adoc'):
            process_file(os.path.join(root, file))

# Write results
with open('big_chunks.txt', 'w') as f:
    for file_path, chunks in results.items():
        f.write(f"File: {file_path}\n")
        for name, length in chunks:
            f.write(f"  - Chunk: {name}, Lines: {length}\n")
        f.write("\n")
