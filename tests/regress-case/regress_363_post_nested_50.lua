local t = {
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
    nil, nil, nil, nil, nil, nil, nil, nil, nil, nil,
}
t.tag = { nil, 41 }
t[3] = 3
assert(#t == 0 and #t.tag == 2 and t.tag[2] == 41)
print("regress_363", #t, #t.tag, t.tag[2], t[3])
