#!/usr/bin/env python3
"""Run: python3 scripts/gen-themes.py > sebastian/src/render/themes_gen.rs (needs ./harness).
Translate the flat/loop/if statements of mermaid theme updateColors() into Rust
`fill_<theme>` functions that only fill variables still unset."""
import re, sys

import os
THEMES = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'harness', 'mermaid', 'packages', 'mermaid', 'src', 'themes')

# ---------------------------------------------------------------- tokenizer
TOK = re.compile(r"""
    (?P<ws>\s+)
  | (?P<com>//[^\n]*|/\*.*?\*/)
  | (?P<num>\d+(?:\.\d+)?)
  | (?P<str>'(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*")
  | (?P<id>[A-Za-z_$][A-Za-z0-9_$]*)
  | (?P<op>\?\?|\?\.|\|\||&&|===|!==|==|!=|<=|>=|\+\+|[-+*/<>!?:.,;(){}\[\]=])
""", re.X | re.S)


def tokenize(src):
    pos, out = 0, []
    while pos < len(src):
        m = TOK.match(src, pos)
        if not m:
            raise SyntaxError('bad char %r at %d' % (src[pos], pos))
        pos = m.end()
        k = m.lastgroup
        if k in ('ws', 'com'):
            continue
        out.append((k, m.group(k)))
    return out


class P:
    def __init__(self, toks):
        self.t = toks
        self.i = 0

    def peek(self, o=0):
        return self.t[self.i + o] if self.i + o < len(self.t) else ('eof', '')

    def next(self):
        x = self.peek()
        self.i += 1
        return x

    def eat(self, v):
        if self.peek()[1] == v and self.peek()[0] in ('op', 'id'):
            self.i += 1
            return True
        return False

    def expect(self, v):
        if not self.eat(v):
            raise SyntaxError('expected %s got %r' % (v, self.peek()))


class Unsupported(Exception):
    pass


# ---------------------------------------------------------------- expression AST
def parse_expr(p):
    return parse_ternary(p)


def parse_ternary(p):
    c = parse_or(p)
    if p.eat('?'):
        a = parse_ternary(p)
        p.expect(':')
        b = parse_ternary(p)
        return ('cond', c, a, b)
    return c


def parse_or(p):
    l = parse_and(p)
    while p.peek()[1] in ('||', '??') and p.peek()[0] == 'op':
        op = p.next()[1]
        r = parse_and(p)
        l = ('or' if op == '||' else 'nullish', l, r)
    return l


def parse_and(p):
    l = parse_eq(p)
    while p.peek()[1] == '&&':
        p.next()
        r = parse_eq(p)
        l = ('and', l, r)
    return l


def parse_eq(p):
    l = parse_rel(p)
    while p.peek()[1] in ('===', '!==', '==', '!=') and p.peek()[0] == 'op':
        op = p.next()[1]
        r = parse_rel(p)
        l = ('eq', op, l, r)
    return l


def parse_rel(p):
    l = parse_add(p)
    while p.peek()[1] in ('<', '>', '<=', '>=') and p.peek()[0] == 'op':
        op = p.next()[1]
        r = parse_add(p)
        l = ('rel', op, l, r)
    return l


def parse_add(p):
    l = parse_mul(p)
    while p.peek()[1] in ('+', '-') and p.peek()[0] == 'op':
        op = p.next()[1]
        r = parse_mul(p)
        l = ('bin', op, l, r)
    return l


def parse_mul(p):
    l = parse_unary(p)
    while p.peek()[1] in ('*', '/') and p.peek()[0] == 'op':
        op = p.next()[1]
        r = parse_unary(p)
        l = ('bin', op, l, r)
    return l


def parse_unary(p):
    if p.peek()[0] == 'op' and p.peek()[1] in ('!', '-', '+'):
        op = p.next()[1]
        return ('un', op, parse_unary(p))
    return parse_postfix(p)


def parse_postfix(p):
    e = parse_primary(p)
    while True:
        if p.peek()[1] == '.' and p.peek()[0] == 'op':
            p.next()
            name = p.next()
            e = ('member', e, name[1])
        elif p.peek()[1] == '?.' and p.peek()[0] == 'op':
            p.next()
            name = p.next()
            e = ('member', e, name[1])  # optional chaining on unsupported objects handled later
        elif p.peek()[1] == '[' and p.peek()[0] == 'op':
            p.next()
            k = parse_expr(p)
            p.expect(']')
            e = ('index', e, k)
        elif p.peek()[1] == '(' and p.peek()[0] == 'op':
            p.next()
            args = []
            if not p.eat(')'):
                while True:
                    args.append(parse_expr(p))
                    if p.eat(','):
                        continue
                    p.expect(')')
                    break
            e = ('call', e, args)
        else:
            return e


def parse_primary(p):
    k, v = p.next()
    if k == 'num':
        return ('num', v)
    if k == 'str':
        return ('str', v[1:-1].replace("\\'", "'"))
    if k == 'id':
        return ('id', v)
    if (k, v) == ('op', '('):
        e = parse_expr(p)
        p.expect(')')
        return ('paren', e)
    if (k, v) == ('op', '{'):
        items = []
        if not p.eat('}'):
            while True:
                key = p.next()[1]
                p.expect(':')
                items.append((key, parse_expr(p)))
                if p.eat(','):
                    if p.eat('}'):
                        break
                    continue
                p.expect('}')
                break
        return ('obj', items)
    raise SyntaxError('unexpected %r' % ((k, v),))


# ---------------------------------------------------------------- codegen
def rs_str(s):
    return '"' + s.replace('\\', '\\\\').replace('"', '\\"') + '"'


def name_of(node, env):
    """Static variable name for this.x / this['..'+i]."""
    if node[0] == 'member' and node[1] == ('id', 'this'):
        return rs_str(node[2]), None
    if node[0] == 'index' and node[1] == ('id', 'this'):
        key = node[2]
        return key_format(key, env)
    raise Unsupported('target %r' % (node,))


def key_format(key, env):
    """Return (format-string, [rust arg exprs]) building the variable name."""
    parts = []

    def flat(n):
        if n[0] == 'bin' and n[1] == '+':
            flat(n[2]); flat(n[3])
        else:
            parts.append(n)
    flat(key)
    fmt, args = '', []
    for n in parts:
        if n[0] == 'str':
            fmt += n[1].replace('{', '{{').replace('}', '}}')
        elif n[0] == 'num':
            fmt += n[1]
        elif n == ('id', 'i'):
            fmt += '{}'; args.append('i')
        elif n[0] == 'paren' and n[1][0] == 'bin' and n[1][2] == ('id', 'i') and n[1][3][0] == 'num':
            fmt += '{}'; args.append('i %s %s' % (n[1][1], n[1][3][1]))
        else:
            raise Unsupported('key part %r' % (n,))
    return fmt, args


def fmt_call(fmt, args):
    return 'format!(%s%s)' % (rs_str(fmt), ''.join(', ' + a for a in args))


def ev(n, env):
    """Rust expression of type String."""
    k = n[0]
    if k == 'paren':
        return ev(n[1], env)
    if k == 'num':
        return 'js_num(%sf64)' % n[1] if '.' in n[1] else 'String::from("%s")' % n[1]
    if k == 'str':
        return 'String::from(%s)' % rs_str(n[1])
    if k == 'id':
        if n[1] == 'i':
            return 'i.to_string()'
        if n[1] == 'undefined':
            return 'String::new()'
        if n[1] == 'true':
            return 'String::from("true")'
        if n[1] == 'false':
            return 'String::new()'
        raise Unsupported('id %s' % n[1])
    if k == 'member':
        if n[1] == ('id', 'this'):
            return 'get(v, %s)' % rs_str(n[2])
        # this.xyChart?.x etc.
        if n[1][0] == 'member' and n[1][1] == ('id', 'this'):
            raise Unsupported('nested %s.%s' % (n[1][2], n[2]))
        raise Unsupported('member %r' % (n,))
    if k == 'index':
        if n[1] == ('id', 'this'):
            fmt, args = key_format(n[2], env)
            return 'get(v, &%s)' % fmt_call(fmt, args)
        raise Unsupported('index')
    if k == 'or' or k == 'nullish':
        return 'js_or(%s, || %s)' % (ev(n[1], env), ev(n[2], env))
    if k == 'and':
        return 'js_and(%s, || %s)' % (ev(n[1], env), ev(n[2], env))
    if k == 'cond':
        return 'if js_truthy(&(%s)) { %s } else { %s }' % (ev(n[1], env), ev(n[2], env), ev(n[3], env))
    if k == 'un' and n[1] == '!':
        return 'js_bool(!js_truthy(&(%s)))' % ev(n[2], env)
    if k == 'eq':
        eq = 'js_eq(&(%s), &(%s))' % (ev(n[2], env), ev(n[3], env))
        return 'js_bool(%s)' % (eq if n[1] in ('===', '==') else '!' + eq)
    if k == 'call':
        f = n[1]
        if f[0] == 'id':
            name = f[1]
            a = n[2]
            if name in ('lighten', 'darken'):
                return '%s(&(%s), %s)' % (name, ev(a[0], env), num(a[1], env) if len(a) > 1 else 'f64::NAN')
            if name == 'invert':
                return 'invert(&(%s))' % ev(a[0], env)
            if name == 'adjust':
                items = a[1][1]
                pairs = ', '.join("('%s', %s)" % (key, num(val, env)) for key, val in items)
                return 'adjust(&(%s), &[%s])' % (ev(a[0], env), pairs)
            if name == 'mkBorder':
                return 'mk_border(&(%s), %s)' % (ev(a[0], env), 'js_truthy(&(%s))' % ev(a[1], env) if len(a) > 1 else 'false')
            if name == 'isDark':
                return 'js_bool(super::khroma::is_dark(&(%s)))' % ev(a[0], env)
            if name == 'rgba':
                return 'rgba(%s)' % ', '.join(num(x, env) for x in a)
        raise Unsupported('call %r' % (f,))
    if k == 'bin':
        if n[1] == '+':
            return 'format!("{}{}", %s, %s)' % (ev(n[2], env), ev(n[3], env))
    raise Unsupported('expr %s' % k)


def num(n, env):
    """Rust f64 expression."""
    if n[0] == 'paren':
        return '(%s)' % num(n[1], env)
    if n[0] == 'num':
        return '%sf64' % n[1] if '.' in n[1] else '%s.0f64' % n[1]
    if n[0] == 'un' and n[1] in '-+':
        return '(%s%s)' % (n[1] if n[1] == '-' else '', num(n[2], env))
    if n[0] == 'bin':
        return '(%s %s %s)' % (num(n[2], env), n[1], num(n[3], env))
    if n[0] == 'cond':
        return 'if js_truthy(&(%s)) { %s } else { %s }' % (ev(n[1], env), num(n[2], env), num(n[3], env))
    if n == ('id', 'i'):
        return '(i as f64)'
    if n[0] in ('member', 'index', 'call'):
        return 'js_f64(&(%s))' % ev(n, env)
    raise Unsupported('num %r' % (n,))


# ---------------------------------------------------------------- statements
def parse_block(p, out, env, skipped):
    """Parse statements until closing brace/eof; append Rust source lines to out."""
    while p.peek()[0] != 'eof' and p.peek()[1] != '}':
        if p.peek() == ('op', ';'):
            p.next()
            continue
        start = p.i
        try:
            stmt(p, out, env, skipped)
        except Unsupported as e:
            # skip tokens to end of statement
            skipped.append(str(e))
            p.i = start
            skip_stmt(p)


def skip_stmt(p):
    depth = 0
    while p.peek()[0] != 'eof':
        k, v = p.next()
        if k == 'op':
            if v in '({[':
                depth += 1
            elif v in ')}]':
                depth -= 1
                if depth < 0:
                    p.i -= 1
                    return
                if depth == 0 and v == '}':
                    # block statement ended (for/if) — stop unless else follows
                    if p.peek()[1] == 'else':
                        continue
                    return
            elif v == ';' and depth == 0:
                return


def stmt(p, out, env, skipped):
    k, v = p.peek()
    if (k, v) == ('id', 'for'):
        p.next(); p.expect('(')
        # let i = A ; i < B ; i++
        p.expect('let'); p.next(); p.expect('=')
        a = parse_expr(p); p.expect(';')
        p.next(); p.expect('<')
        b = parse_expr(p); p.expect(';')
        while not p.eat(')'):
            p.next()
        lo = static_int(a)
        hi = static_int(b)
        p.expect('{')
        body, sk = [], []
        parse_block(p, body, {'loop': True}, sk)
        p.expect('}')
        skipped.extend(sk)
        out.append('for i in %d..%d {' % (lo, hi))
        out.extend(body)
        out.append('}')
        return
    if (k, v) == ('id', 'if'):
        p.next(); p.expect('(')
        c = parse_expr(p); p.expect(')')
        cond = ev(c, env)
        p.expect('{')
        a_body, sk = [], []
        parse_block(p, a_body, env, sk); p.expect('}')
        b_body = []
        if p.eat('else'):
            if p.peek()[1] == 'if':
                parse_block_single_if(p, b_body, env, sk)
            else:
                p.expect('{'); parse_block(p, b_body, env, sk); p.expect('}')
        skipped.extend(sk)
        out.append('if js_truthy(&{ let v = &*vars; %s }) {' % cond)
        out.extend(a_body)
        out.append('} else {')
        out.extend(b_body)
        out.append('}')
        return
    if (k, v) == ('id', 'const') or (k, v) == ('id', 'let'):
        raise Unsupported('declaration')
    # assignment: this.x = expr ;
    target = parse_postfix_no_call(p)
    p.expect('=')
    if p.peek()[1] == '{':
        raise Unsupported('object assignment')
    e = parse_expr(p)
    p.eat(';')
    t = target
    if t[0] == 'member' and t[1] == ('id', 'this'):
        keyexpr = rs_str(t[2])
    elif t[0] == 'index' and t[1] == ('id', 'this'):
        fmt, args = key_format(t[2], env)
        keyexpr = '&' + fmt_call(fmt, args)
    else:
        raise Unsupported('target %r' % (t,))
    # `this.x = this.x || rest` only fills a still-unset variable.
    while e[0] == 'paren':
        e = e[1]
    if e[0] in ('or', 'nullish') and e[1] == t:
        out.append('fill(vars, %s, |v| %s);' % (keyexpr, ev(e[2], env)))
    else:
        out.append('set_var(vars, %s, |v| %s);' % (keyexpr, ev(e, env)))


def parse_block_single_if(p, out, env, sk):
    # else-if chain: emit nested
    tmp = []
    stmt(p, tmp, env, sk)
    out.extend(tmp)


def parse_postfix_no_call(p):
    e = parse_primary(p)
    while True:
        if p.peek()[1] == '.' and p.peek()[0] == 'op':
            p.next(); e = ('member', e, p.next()[1])
        elif p.peek()[1] == '[' and p.peek()[0] == 'op':
            p.next(); k = parse_expr(p); p.expect(']'); e = ('index', e, k)
        else:
            return e


def static_int(n):
    if n[0] == 'num':
        return int(n[1])
    if n == ('member', ('id', 'this'), 'THEME_COLOR_LIMIT'):
        return 12
    raise Unsupported('loop bound %r' % (n,))


def method_body(src, name):
    i = src.index(name + '() {')
    j = src.index('{', i)
    depth, k = 0, j
    while True:
        c = src[k]
        if c == '{':
            depth += 1
        elif c == '}':
            depth -= 1
            if depth == 0:
                break
        k += 1
    return src[j + 1:k]


def gen(theme, method, name):
    src = open(os.path.join(THEMES, 'theme-%s.js' % theme)).read()
    body = method_body(src, method)
    p = P(tokenize(body))
    out, skipped = [], []
    parse_block(p, out, {}, skipped)
    fn = 'pub(super) fn %s_%s(vars: &mut Vars) {\n    let dm = dark_mode(vars);\n    let _ = dm;\n' % (name, theme)
    fn += '\n'.join('    ' + l for l in out) + '\n}\n'
    return fn, skipped


if __name__ == '__main__':
    themes = sys.argv[1:] or ['dark', 'forest', 'base', 'neutral']
    code = '''//! GENERATED by scripts/gen-themes.py from mermaid's theme-*.js
//! `updateColors()` bodies. Each statement only fills a variable that is
//! still unset, so it complements the hand-written ports in `themes.rs`.

#![allow(unused_variables, unused_imports, clippy::all, clippy::pedantic, clippy::nursery)]

use super::khroma::{adjust, darken, invert, lighten, mk_border, rgba};
use super::themes::{
    dark_mode, fill, get, js_and, js_bool, js_eq, js_or, js_truthy, set_var,
};

type Vars = serde_json::Map<String, serde_json::Value>;

'''
    for t in themes:
        for method, name in (('constructor', 'ctor'), ('updateColors', 'fill')):
            fn, sk = gen(t, method, name)
            code += fn + '\n'
            print('%s %s: %d skipped' % (t, name, len(sk)), file=sys.stderr)
            for x in sorted(set(sk)):
                print('   skip:', x, file=sys.stderr)
    sys.stdout.write(code)
