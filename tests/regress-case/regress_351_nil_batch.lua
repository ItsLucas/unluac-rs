-- Preserve the original nested SETLIST layout and later length observations.
local rows = {
    { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } },
    { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } },
    { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } },
    { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } },
    { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } },
    { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } },
    { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } },
    { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } },
    { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } },
    { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } },
    { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } },
    { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } },
    { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } }, { { nil, 2 }, { 3, nil } },
}
assert(#rows == 65)
rows[8] = nil
assert(#rows == 65)
assert(#rows[1] == 2 and rows[1][1][2] == 2 and rows[1][2][1] == 3)
print("regress_351", #rows, #rows[1])
