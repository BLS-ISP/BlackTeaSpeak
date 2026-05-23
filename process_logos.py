import os
import re
from PIL import Image

def process_image(img_path):
    try:
        img = Image.open(img_path).convert("RGBA")
        data = img.getdata()
        
        new_data = []
        changed = False
        
        for r, g, b, a in data:
            # Check if pixel is predominantly red
            # A simple heuristic: red is dominant if r is significantly larger than g and b
            if a > 0 and r > 100 and r > g + 40 and r > b + 40:
                # Convert to grayscale
                gray = int((r * 0.299) + (g * 0.587) + (b * 0.114))
                # Map to dark gray/black to match "BlackTeaSpeak"
                dark_gray = int(gray * 0.4) 
                new_data.append((dark_gray, dark_gray, dark_gray, a))
                changed = True
            else:
                new_data.append((r, g, b, a))
        
        if changed:
            img.putdata(new_data)
            img.save(img_path)
            print(f"Processed image: {img_path}")
    except Exception as e:
        print(f"Failed to process {img_path}: {e}")

def process_svg(svg_path):
    try:
        with open(svg_path, 'r', encoding='utf-8') as f:
            content = f.read()
        
        # Replace common red colors in SVGs
        new_content = re.sub(r'#ff0000', '#222222', content, flags=re.IGNORECASE)
        new_content = re.sub(r'#e53935', '#222222', content, flags=re.IGNORECASE)
        new_content = re.sub(r'#f44336', '#222222', content, flags=re.IGNORECASE)
        new_content = re.sub(r'red', 'black', new_content, flags=re.IGNORECASE)
        
        if content != new_content:
            with open(svg_path, 'w', encoding='utf-8') as f:
                f.write(new_content)
            print(f"Processed SVG: {svg_path}")
    except Exception as e:
        pass

def main():
    workspace_dir = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server"
    exclude_dirs = {'.git', '.venv', '.vscode', 'node_modules', 'target', 'dist', 'vendor', 'target-validation'}

    for root, dirs, files in os.walk(workspace_dir):
        dirs[:] = [d for d in dirs if d not in exclude_dirs]
        for f in files:
            if f.endswith('.png'):
                process_image(os.path.join(root, f))
            elif f.endswith('.svg'):
                process_svg(os.path.join(root, f))

if __name__ == "__main__":
    main()
