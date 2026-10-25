import subprocess,sys,re
D='sebastian/tests/pele_cases'
t=sys.argv[1]
for l in open(f'{D}/KNOWN_FAILURES.txt'):
    l=l.strip()
    if not l.startswith(t+'/'): continue
    r=subprocess.run(['./target/debug/seb','-i',f'{D}/{l}.mmd','-o','/tmp/o.svg'],capture_output=True,text=True)
    if r.returncode: print(l.split('/')[1],'ERR',r.stderr.strip()[:110]); continue
    a=open('/tmp/o.svg').read(); b=open(f'{D}/{l}.svg').read()
    va=re.search(r'viewBox="([^"]*)"',a); vb=re.search(r'viewBox="([^"]*)"',b)
    print(l.split('/')[1],'VB' if va and vb and va.group(1)!=vb.group(1) else 'body', va and va.group(1), vb and vb.group(1))
