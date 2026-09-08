-- NEWTABLE reserves hash space even though SETLIST flushes before this record.
local t = {
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    tag = 17,
}
t[3] = 3
assert(#t == 3 and t.tag == 17)
print("regress_358", #t, t.tag, t[3])
