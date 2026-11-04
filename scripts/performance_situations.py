#!/usr/bin/env python3
"""Generate standalone retained-realm JavaScript situations with checked inputs.

Only prepare(input) constructs data. Each timed run() prints one checksum and
returns undefined; no parser, evaluator or timing implementation is embedded.
"""
import argparse
import json
from pathlib import Path

SEEDS = (3, 5, 7, 11)
GLOBAL_STEPS = 1024
FRAME_STEPS = 512
FRAME_CALLS = 16
ARGUMENT_CALLS = 1024
PROPERTY_READS = 8192


def records(seed):
    return [(seed + i, seed * 3 + i * 2, (seed + i) & 3) for i in range(64)]


def frame_checksum(seed):
    rows = records(seed)
    tick, total = seed, 0
    for _ in range(FRAME_CALLS):
        for i in range(FRAME_STEPS):
            x, y, mask = rows[i & 63]
            if i & 1:
                tick = (tick + 3) & 255
                value = y + tick
                if mask & 2:
                    value ^= x
            else:
                tick = (tick + 1) & 255
                value = x + tick
                if mask & 1:
                    value ^= y
            total += value & 1023
    return total


def arguments_checksum(seed):
    rows = records(seed)
    tick, total = seed, 0
    for i in range(ARGUMENT_CALLS):
        x, y, _ = rows[i & 63]
        value = i + tick
        tick = (tick + 1) & 255
        total += ((x ^ value) + y + 2 + tick) & 1023
    return total


def property_checksum(seed):
    rows = records(seed)
    total = 0
    for i in range(PROPERTY_READS):
        x, y, _ = rows[i & 63]
        if i & 1:
            total += y
        else:
            total += x
    return total


ROW_SETUP = '''rows = [];
  for (var i = 0; i < 64; i++) {
    rows.push({x: input + i, y: input * 3 + i * 2, mask: (input + i) & 3});
  }'''


def global_source(kind):
    deep = kind != 'shallow'
    getter = kind == 'getter-deep'
    bound = kind == 'bound-deep'
    names = ('Map', 'Set', 'Uint8Array', 'DataView', 'Date', 'Number')
    captures = '\n'.join(f'var saved{name} = {name};' for name in names)
    prepare = '''expectedMap = savedMap;'''
    if getter:
        prepare = '''mapValue = savedMap;
  if (input & 2) mapValue = alternateMap;
  expectedMap = mapValue;
  getterCount = 0;
  Object.defineProperty(globalThis, 'Map', {
    configurable: true,
    get: function () { getterCount++; return mapValue; }
  });'''
    comparisons = []
    nearby_captures = []
    for name in names:
        actual = name
        captured = 'saved' + name
        if name == 'Map':
            captured = 'expectedMap'
        expected = 'e' + name
        nearby_captures.append(f'var {expected} = {captured};')
        if bound:
            actual = 'b' + name
            nearby_captures.append(f'var {actual} = {expected};')
        comparisons.append(f'if ({actual} === {expected}) sum++;')
    nearby_captures = '\n'.join(nearby_captures)
    body = '''var sum = 0;
    for (var i = 0; i < 1024; i++) {
      sum += PADDING;
      COMPARISONS
    }
    return sum;'''
    body = body.replace('COMPARISONS', '\n      '.join(comparisons))
    if deep:
        body = body.replace('PADDING', ' + '.join(f'p{i}' for i in range(1, 9)))
        nested = nearby_captures + '\nreturn function () {\n    ' + body + '\n  };'
        for level in range(8, 0, -1):
            nested = f'var p{level} = input + {level};\n' + nested
            if level > 1:
                nested = 'return (function () {\n' + nested + '\n})();'
        factory = 'function make(input) {\n' + nested + '\n}'
    else:
        body = body.replace('PADDING', 'padding')
        factory = ('function make(input) {\n  var padding = input * 8 + 36;\n' +
                   nearby_captures + '\n  return function () {\n' + body + '\n  };\n}')
    finish = 'print(worker());'
    if getter:
        finish = 'var sum = worker();\n  print(sum + getterCount);'
    return f'''{captures}
var worker, expectedMap, mapValue, getterCount;
function alternateMap() {{}}
{factory}
export function prepare(input) {{
  {prepare}
  worker = make(input);
}}
export function run() {{
  {finish}
}}
'''


def frame_source(sites):
    if sites not in (1, 32, 128, 512):
        raise ValueError('frame sites must be 1, 32, 128 or 512')
    if sites == 1:
        body = 'return steps[index & 1](row);'
    else:
        lines = [f'case {site}: return steps[{site & 1}](row);' for site in range(sites)]
        body = f'switch (index & {sites - 1}) {{\n    ' + '\n    '.join(lines) + '\n  }'
    return f'''var rows, steps;
export function prepare(input) {{
  {ROW_SETUP}
  var tick = input;
  steps = [function (row) {{
    tick = (tick + 1) & 255;
    var value = row.x + tick;
    if (row.mask & 1) value = value ^ row.y;
    return value & 1023;
  }}, function (row) {{
    tick = (tick + 3) & 255;
    var value = row.y + tick;
    if (row.mask & 2) value = value ^ row.x;
    return value & 1023;
  }}];
}}
function frame(index, row) {{
  {body}
}}
export function run() {{
  var sum = 0;
  for (var call = 0; call < 16; call++) {{
    for (var i = 0; i < 512; i++) sum += frame(i, rows[i & 63]);
  }}
  print(sum);
}}
'''


def arguments_source(observed):
    base, length = 'original', '2'
    if observed:
        base, length = 'arguments[0]', 'arguments.length'
    return f'''var rows, step;
export function prepare(input) {{
  {ROW_SETUP}
  var tick = input;
  step = function (value, index) {{
    'use strict';
    var original = value;
    value = index + tick;
    tick = (tick + 1) & 255;
    var record = {base};
    return ((record.x ^ value) + record.y + {length} + tick) & 1023;
  }};
}}
export function run() {{
  var sum = 0;
  for (var i = 0; i < 1024; i++) sum += step(rows[i & 63], i);
  print(sum);
}}
'''


def property_source(mutated):
    mutation = ''
    if mutated:
        mutation = '''
  for (var i = 0; i < rows.length; i++) {
    var row = rows[i];
    row.temporary = i;
    Object.defineProperty(row, 'x', {configurable: true, enumerable: true,
      get: (function (value) { return function () { return value; }; })(input + i)});
    var value = row.x;
    delete row.temporary;
    delete row.x;
    Object.defineProperty(row, 'x', {value: value, writable: true,
      configurable: true, enumerable: true});
  }
'''
    return f'''var rows, keys;
export function prepare(input) {{
  {ROW_SETUP}
  keys = ['x', 'y', 'prefix-x'.slice(7), String.fromCharCode(121)];
  {mutation}
}}
export function run() {{
  var sum = 0;
  for (var i = 0; i < 8192; i++) sum += rows[i & 63][keys[i & 3]];
  print(sum);
}}
'''


def function_property_source(kind):
    targets = '[Array, Map, Set, Date, Number, String, Boolean, Uint8Array]'
    if kind == 'ordinary':
        targets = '''[{length: 1, name: 'Array'}, {length: 0, name: 'Map'},
    {length: 0, name: 'Set'}, {length: 7, name: 'Date'},
    {length: 1, name: 'Number'}, {length: 1, name: 'String'},
    {length: 1, name: 'Boolean'}, {length: 3, name: 'Uint8Array'}]'''
    elif kind == 'function':
        targets = '''[function Array(a) {}, function Map() {}, function Set() {},
    function Date(a, b, c, d, e, f, g) {}, function Number(a) {},
    function String(a) {}, function Boolean(a) {}, function Uint8Array(a, b, c) {}]'''
    elif kind != 'intrinsic':
        raise ValueError('function property kind must be ordinary, function or intrinsic')
    return f'''var rows, seed;
export function prepare(input) {{
  seed = input;
  var targets = {targets};
  rows = [];
  for (var i = 0; i < 64; i++) rows.push(targets[(input + i) & 7]);
}}
export function run() {{
  var sum = seed;
  for (var i = 0; i < 8192; i++) {{
    var row = rows[i & 63];
    sum += row.length + row.name.length;
  }}
  print(sum);
}}
'''


def workloads():
    entries = []

    def add(name, source, checksum, category, scope, operations, label, **metadata):
        invocations = [dict(input=seed, stdout=str(checksum(seed)) + '\n') for seed in SEEDS]
        entries.append(dict(file=name + '.js', source=source, label=label,
                            category=category, scope=scope, operations=operations,
                            stdout=invocations[0]['stdout'], invocations=invocations,
                            **metadata))

    add('control-empty', '''var input;
export function prepare(value) { input = value; }
export function run() { print(input); }
''', lambda seed: seed, 'control', 'one retained exported call and checksum print', 1,
        'Empty exported-call control')
    for kind in ('shallow', 'deep', 'bound-deep', 'getter-deep'):
        def checksum(seed, getter=kind == 'getter-deep'):
            total = GLOBAL_STEPS * (seed * 8 + 36 + 6)
            if getter:
                total += GLOBAL_STEPS
            return total
        name = kind
        if kind == 'deep':
            name = 'deep8'
        add('global-' + name, global_source(kind), checksum, 'global lookup',
            '1024 passes; six constructor values; eight padding captures contribute to checksum',
            GLOBAL_STEPS * 6, 'Global references: ' + name)
    for sites in (1, 32, 128, 512):
        name = 'frame-compact'
        if sites != 1:
            name = f'frame-branches{sites}'
        add(name, frame_source(sites), frame_checksum, 'frame size',
            '8192 frame entries and ordered closure steps; all source sites exercised; dispatch and static/dynamic step keys differ; not isolated probe cost',
            FRAME_STEPS * FRAME_CALLS, 'Frame selected-branch source sites: ' + str(sites),
            source_sites=sites, frame_entries=FRAME_STEPS * FRAME_CALLS)
    for observed in (False, True):
        name = 'args-unused'
        label = 'Strict ordinary function: original parameter snapshot'
        if observed:
            name = 'args-observed'
            label = 'Strict ordinary function: observed arguments after formal reassignment'
        add(name, arguments_source(observed), arguments_checksum, 'arguments observation',
            '1024 ordinary strict calls; equivalent result; arguments observation adds work, not isolated allocation cost',
            ARGUMENT_CALLS, label)
    for mutated in (False, True):
        name = 'property-stable'
        if mutated:
            name = 'property-mutated'
        add(name, property_source(mutated), property_checksum, 'property mutation history',
            '8192 identical reads of 64 records after prepare; alternating literal and constructed keys; no string-backing claim',
            PROPERTY_READS, name.replace('-', ' '))
    for kind in ('ordinary', 'function', 'intrinsic'):
        add('metadata-' + kind, function_property_source(kind),
            lambda seed: seed + PROPERTY_READS // 8 * 58, 'function properties',
            '8192 passes; own length and name plus string length; eight prepared targets; equivalent values across object kinds',
            PROPERTY_READS * 3, 'Metadata reads: ' + kind)
    return entries


def write_corpus(destination):
    entries = workloads()
    destination.mkdir(parents=True, exist_ok=False)
    for entry in entries:
        (destination / entry['file']).write_text(entry['source'], encoding='utf-8')
    plan = dict(schema='scriptw-retained-situations-v1', source_format='module',
                execution=[{key: value for key, value in entry.items() if key != 'source'}
                           for entry in entries])
    (destination / 'manifest.json').write_text(json.dumps(plan, indent=2) + '\n', encoding='utf-8')
    return plan


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()
    try:
        plan = write_corpus(args.output_dir)
    except (OSError, ValueError) as error:
        parser.exit(1, f'performance-situations: {error}\n')
    print(f"Wrote {len(plan['execution'])} checked retained situations to {args.output_dir}")


if __name__ == '__main__':
    main()
