const fs = require('fs');
const path = require('path');

function checkDir(dir) {
    const files = fs.readdirSync(dir);
    for (const f of files) {
        const fullPath = path.join(dir, f);
        if (fs.statSync(fullPath).isDirectory()) {
            checkDir(fullPath);
        } else if (f.endsWith('.ts') || f.endsWith('.tsx')) {
            const content = fs.readFileSync(fullPath, 'utf8');
            const imports = [...content.matchAll(/import.*?from\s+['"]([^'"]+)['"]/g)].map(m => m[1]);
            for (const imp of imports) {
                if (imp.startsWith('.')) {
                    // check if file exists with EXACT case
                    const targetPath = path.resolve(path.dirname(fullPath), imp);
                    // try appending .ts, .tsx, .d.ts, /index.ts, /index.tsx
                    const exts = ['', '.ts', '.tsx', '/index.ts', '/index.tsx', '.css'];
                    let found = false;
                    for (const ext of exts) {
                        const testPath = targetPath + ext;
                        if (fs.existsSync(testPath)) {
                            // Check exact case
                            const dirName = path.dirname(testPath);
                            const baseName = path.basename(testPath);
                            const actualFiles = fs.readdirSync(dirName);
                            if (actualFiles.includes(baseName)) {
                                found = true;
                                break;
                            }
                        }
                    }
                    if (!found) {
                        console.log(`Mismatch in ${fullPath}: import '${imp}' not found with exact case`);
                    }
                }
            }
        }
    }
}

checkDir('d:/projekt/BlackTeaSpeak/BlackTeaSpeak-Client/src');
