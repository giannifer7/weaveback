# scripts/gliner_experiment.py
import sys
import os
from gliner import GLiNER

def main():
    # Load GLiNER model (using a widely supported base model for verification)
    print("Loading GLiNER model (numind/gliner-base-v1)...", file=sys.stderr)
    model = GLiNER.from_pretrained("numind/gliner-base-v1")

    # Path to architecture docs
    arch_doc_path = "docs/architecture.adoc"
    if not os.path.exists(arch_doc_path):
        print(f"Error: {arch_doc_path} not found.", file=sys.stderr)
        return

    with open(arch_doc_path, "r") as f:
        text = f.read()

    # Semantic labels based on "Intent and Constraint" extraction
    labels = [
        "Constraint",
        "Invariant",
        "Dependency Relation",
        "Intent",
        "Architectural Component"
    ]

    # Process in smaller chunks (paragraphs) for higher precision
    paragraphs = [p.strip() for p in text.split("\n\n") if p.strip()]
    
    found = {}
    print(f"Extracting semantic links from {len(paragraphs)} paragraphs...", file=sys.stderr)
    
    for p in paragraphs:
        # Lower threshold for exploration, but we'll print confidence
        entities = model.predict_entities(p, labels, threshold=0.3)
        for entity in entities:
            label = entity["label"]
            text = entity["text"].strip()
            score = entity["score"]
            
            if label not in found:
                found[label] = {}
            
            # Keep highest score for each unique text
            if text not in found[label] or score > found[label][text]:
                found[label][text] = score

    print("\n--- GLiNER Semantic Extraction Results ---\n")
    for label in sorted(found.keys()):
        print(f"[{label}]")
        # Sort by confidence
        sorted_items = sorted(found[label].items(), key=lambda x: x[1], reverse=True)
        for item, score in sorted_items:
            print(f"  - {item:<50} (conf: {score:.2f})")
        print()

if __name__ == "__main__":
    main()
