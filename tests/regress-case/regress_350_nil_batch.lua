-- Preserve the original nested SETLIST layout and later length observations.
local rows = {
    { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil },
    { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil },
    { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil },
    { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil },
    { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil },
    { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil },
    { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil },
    { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil },
    { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil },
    { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil }, { nil, nil, nil },
}
assert(#rows == 50)
rows[8] = nil
assert(#rows == 50)
assert(#rows[1] == 0 and rows[1][1] == nil and rows[1][3] == nil)
rows[1][3] = 3
assert(#rows[1] == 3, "all-nil constructor must retain its array capacity")
print("regress_350", #rows, #rows[1])
