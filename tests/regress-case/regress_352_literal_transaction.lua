-- Scalar fields of later siblings overwrite earlier record-table scratch homes.
local rows = {
    { 1, 2, 3, tag = { nil, 5 } },
    { 1, 2, 3, tag = { nil, 7 } },
    { 1, 2, 3, tag = { nil, 9 } },
    { 1, 2, 3, tag = { nil, 11 } },
    { 1, 2, 3, tag = { nil, 13 } },
    { 1, 2, 3, tag = { nil, 15 } },
    { 1, 2, 3, tag = { nil, 17 } },
    { 1, 2, 3, tag = { nil, 19 } },
    { 1, 2, 3, tag = { nil, 21 } },
}
assert(#rows == 9 and #rows[1].tag == 2)
for i = 1, 9 do
    assert(rows[i][3] == 3 and rows[i].tag[1] == nil)
    assert(rows[i].tag[2] == i * 2 + 3)
end
rows[2] = nil
print("regress_352", #rows)
rows[4] = nil
print(#rows)
rows[6] = nil
print(#rows)
rows[8] = nil
print(#rows)
rows[9] = nil
print(#rows)
