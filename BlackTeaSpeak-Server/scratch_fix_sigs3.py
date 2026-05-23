import re

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

# Replace any function declaration that fails to close its parentheses before the next fn
# A broken one looks like:
# fn name(
#     ...
# fn next_name
# So if we find `fn ` followed by anything up to the next `fn ` that doesn't contain `{`
# we delete it.
# Actually, wait, it could be `fn ` then some args, then `fn ` immediately.
while True:
    new_content = re.sub(r"^\s*fn [a-zA-Z0-9_]+\(\n\s*\&(?:mut )?self,\n\s*(?:fn\s|match\s|let\s|Ok|return)", r"\n    \1", content, flags=re.MULTILINE)
    new_content = re.sub(r"^\s*fn [a-zA-Z0-9_]+\(\n\s*\&(?:mut )?self,\n\n+(?:fn\s|match\s|let\s|Ok|return)", r"\n    \1", new_content, flags=re.MULTILINE)
    
    # Also handle things that have 1 argument after &self, like `fn handle_ban_client(\n &mut self,\n fn`
    
    # A generic regex: find `fn name(` then `&self` or `&mut self` and maybe some lines without `)`, then immediately another `fn`
    new_content = re.sub(r"^[ \t]*fn [a-zA-Z0-9_]+\([^\)]+?^[ \t]*fn ", r"    fn ", new_content, flags=re.MULTILINE)

    if new_content == content:
        break
    content = new_content

with open(filepath, "w", encoding="utf-8") as f:
    f.write(content)
