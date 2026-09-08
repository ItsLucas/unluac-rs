-- The same writes without a hash reservation intentionally shrink the nil array.
local t = {
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
}
t.tag = 17
t[3] = 3
assert(#t == 0 and t.tag == 17)
print("regress_359", #t, t.tag, t[3])
