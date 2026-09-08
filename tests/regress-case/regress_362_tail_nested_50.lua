-- Recover the trailing allocation and its own nil-containing array atomically.
local t = {
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    tag = { nil, 41 },
}
t[3] = 3
assert(#t == 3 and #t.tag == 2 and t.tag[2] == 41)
print("regress_362", #t, #t.tag, t.tag[2], t[3])
