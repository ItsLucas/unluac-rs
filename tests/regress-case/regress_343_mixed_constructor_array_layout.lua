-- Literal producers force slot reuse; later holes expose array/hash layout changes.
local rows = {
    {1,2,3,4,5,6,7,8,9,tag=true}, {1,2,3,4,5,6,7,8,9,tag=true},
    {1,2,3,4,5,6,7,8,9,tag=true}, {1,2,3,4,5,6,7,8,9,tag=true},
    {1,2,3,4,5,6,7,8,9,tag=true}, {1,2,3,4,5,6,7,8,9,tag=true},
    {1,2,3,4,5,6,7,8,9,tag=true}, {1,2,3,4,5,6,7,8,9,tag=true},
    {1,2,3,4,5,6,7,8,9,tag=true}, {1,2,3,4,5,6,7,8,9,tag=true},
    {1,2,3,4,5,6,7,8,9,tag=true}, {1,2,3,4,5,6,7,8,9,tag=true},
    {1,2,3,4,5,6,7,8,9,tag=true}, {1,2,3,4,5,6,7,8,9,tag=true},
    {1,2,3,4,5,6,7,8,9,tag=true}, {1,2,3,4,5,6,7,8,9,tag=true},
    {1,2,3,4,5,6,7,8,9,tag=true}, {1,2,3,4,5,6,7,8,9,tag=true},
    {1,2,3,4,5,6,7,8,9,tag=true}, {1,2,3,4,5,6,7,8,9,tag=true},
    {1,2,3,4,5,6,7,8,9,tag=true}, {1,2,3,4,5,6,7,8,9,tag=true},
    {1,2,3,4,5,6,7,8,9,tag=true}, {1,2,3,4,5,6,7,8,9,tag=true},
    {1,2,3,4,5,6,7,8,9,tag=true},
}
local function inspect()
    rows[2][8] = nil
    print("regress_343", #rows, #rows[2])
    assert(#rows[2] == 9)
end
inspect()
