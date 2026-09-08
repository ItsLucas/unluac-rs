-- The same transaction must include the flush at 50 and the reused scratch homes.
local rows = {
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } },
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } },
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } },
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } },
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } },
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } },
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } },
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } },
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } },
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } },
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } },
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } },
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 5 } },
}
assert(#rows == 65)
for i = 1, 65 do
    assert(rows[i][3] == 3 and #rows[i].tag == 2 and rows[i].tag[2] == 5)
end
for i = 8, 64, 8 do
    rows[i] = nil
    assert(#rows == 65, "partial-batch recovery changed the outer array layout")
end
rows[65] = nil
print("regress_353", #rows)
