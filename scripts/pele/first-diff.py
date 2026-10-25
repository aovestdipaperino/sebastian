import subprocess,sys,re
D='sebastian/tests/pele_cases'
n=sys.argv[1]
subprocess.run(['./target/debug/seb','-i',f'{D}/{n}.mmd','-o','/tmp/dd.svg'])
a=open('/tmp/dd.svg').read(); b=open(f'{D}/{n}.svg').read()
# strip style
a=re.sub(r'<style>.*?</style>','<style/>',a,flags=re.S); b=re.sub(r'<style>.*?</style>','<style/>',b,flags=re.S)
i=next((i for i,(x,y) in enumerate(zip(a,b)) if x!=y),min(len(a),len(b)))
W=int(sys.argv[2]) if len(sys.argv)>2 else 150
print('OURS:',a[max(0,i-60):i+W]);print('REF :',b[max(0,i-60):i+W])
