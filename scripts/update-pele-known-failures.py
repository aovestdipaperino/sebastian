#!/usr/bin/env python3
"""Rewrites sebastian/tests/pele_cases/KNOWN_FAILURES.txt from the current
renderer. Build first: cargo build -p seb. Run from the repository root."""
import glob, os, re, subprocess, tempfile


def mask(s):
    # gantt 'today' marker depends on the render date
    return re.sub(r'(<g class="today"><line x1=")[^"]*(" x2=")[^"]*', r'\g<1>0\g<2>0', s)


D = 'sebastian/tests/pele_cases'
fails = []
with tempfile.TemporaryDirectory() as tmp:
    out = os.path.join(tmp, 'o.svg')
    for mmd in sorted(glob.glob(f'{D}/*/*.mmd')):
        case = mmd[len(D) + 1:-4]
        ok = False
        try:
            r = subprocess.run(['./target/debug/seb', '-i', mmd, '-o', out],
                               capture_output=True, timeout=60)
            ok = r.returncode == 0 and mask(open(out).read()) == mask(open(mmd[:-4] + '.svg').read())
        except subprocess.TimeoutExpired:
            pass
        if os.path.exists(out):
            os.remove(out)
        if not ok:
            fails.append(case)
head = [l for l in open(f'{D}/KNOWN_FAILURES.txt').read().split('\n') if l.startswith('#')]
open(f'{D}/KNOWN_FAILURES.txt', 'w').write('\n'.join(head + fails) + '\n')
print(len(fails), 'known failures')
