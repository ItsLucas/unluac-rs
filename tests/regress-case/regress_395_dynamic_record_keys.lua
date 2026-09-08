local count = 0
local function key()
    count = count + 1
    return 0
end
local t = {
    [key()] = { 11, 22 },
    [1] = { 33, 44 },
    ["0"] = { 55, 66 },
}
assert(count == 1)
assert(t[0][2] == 22 and t[1][2] == 44 and t["0"][2] == 66)
print("regress_395", count, t[0][1], t[1][1], t["0"][1])
