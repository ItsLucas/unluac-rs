-- A completed nested literal must win before the indexed SETLIST fallback.
-- Later holes expose the outer array capacity; inner nils are intentional.
local rows = {
    { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 },
    { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 },
    { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 },
    { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 },
    { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 },
    { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 },
    { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 },
    { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 },
    { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 },
    { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 },
    { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 },
    { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 },
    { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 }, { nil, 2, 3 },
}
rows[8] = nil
assert(#rows == 65, "outer SETLIST layout must survive")
assert(rows[1][1] == nil and rows[1][3] == 3 and #rows[1] == 3)
print("regress_348", #rows, #rows[1], rows[65][3])
