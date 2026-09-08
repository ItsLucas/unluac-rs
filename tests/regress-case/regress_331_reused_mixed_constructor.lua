-- Reused constructor slots must retain their scalar snapshots and record writes.
-- Keep this literal: the 250 scalar producers exercise source-local pressure.
local rows = {
    { 1, 2, 3, 4, 5, tag = true }, { 2, 3, 4, 5, 6, tag = false },
    { 3, 4, 5, 6, 7, tag = true }, { 4, 5, 6, 7, 8, tag = false },
    { 5, 6, 7, 8, 9, tag = true }, { 6, 7, 8, 9, 10, tag = false },
    { 7, 8, 9, 10, 11, tag = true }, { 8, 9, 10, 11, 12, tag = false },
    { 9, 10, 11, 12, 13, tag = true }, { 10, 11, 12, 13, 14, tag = false },
    { 11, 12, 13, 14, 15, tag = true }, { 12, 13, 14, 15, 16, tag = false },
    { 13, 14, 15, 16, 17, tag = true }, { 14, 15, 16, 17, 18, tag = false },
    { 15, 16, 17, 18, 19, tag = true }, { 16, 17, 18, 19, 20, tag = false },
    { 17, 18, 19, 20, 21, tag = true }, { 18, 19, 20, 21, 22, tag = false },
    { 19, 20, 21, 22, 23, tag = true }, { 20, 21, 22, 23, 24, tag = false },
    { 21, 22, 23, 24, 25, tag = true }, { 22, 23, 24, 25, 26, tag = false },
    { 23, 24, 25, 26, 27, tag = true }, { 24, 25, 26, 27, 28, tag = false },
    { 25, 26, 27, 28, 29, tag = true }, { 26, 27, 28, 29, 30, tag = false },
    { 27, 28, 29, 30, 31, tag = true }, { 28, 29, 30, 31, 32, tag = false },
    { 29, 30, 31, 32, 33, tag = true }, { 30, 31, 32, 33, 34, tag = false },
    { 31, 32, 33, 34, 35, tag = true }, { 32, 33, 34, 35, 36, tag = false },
    { 33, 34, 35, 36, 37, tag = true }, { 34, 35, 36, 37, 38, tag = false },
    { 35, 36, 37, 38, 39, tag = true }, { 36, 37, 38, 39, 40, tag = false },
    { 37, 38, 39, 40, 41, tag = true }, { 38, 39, 40, 41, 42, tag = false },
    { 39, 40, 41, 42, 43, tag = true }, { 40, 41, 42, 43, 44, tag = false },
    { 41, 42, 43, 44, 45, tag = true }, { 42, 43, 44, 45, 46, tag = false },
    { 43, 44, 45, 46, 47, tag = true }, { 44, 45, 46, 47, 48, tag = false },
    { 45, 46, 47, 48, 49, tag = true }, { 46, 47, 48, 49, 50, tag = false },
    { 47, 48, 49, 50, 51, tag = true }, { 48, 49, 50, 51, 52, tag = false },
    { 49, 50, 51, 52, 53, tag = true }, { 50, 51, 52, 53, 54, tag = false },
}
local function inspect()
    local total = 0
    for i = 1, #rows do
        total = total + rows[i][1] + rows[i][5]
        assert(rows[i].tag == (i % 2 == 1))
    end
    print("regress_331", #rows, total)
end
inspect()
