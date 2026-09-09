-- A full constant pool materializes primitives even when their keys are reused.
-- unluac: expect-not-contains [[unluac error]]
local reads = 0
Record416Catalog = setmetatable({}, {__index = function(_, key)
    assert(Record416Rows == "old")
    reads = reads + 1
    if Record416Fail == reads then error("primitive-stop", 0) end
    return reads
end})
function Record416Pad(...) end
function Record416Build()
    Record416Pad("enabled", "disabled", "cleared", "padding")
    Record416Pad(
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
        16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
        32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
        48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63)
    Record416Pad(
        64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79,
        80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95,
        96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111,
        112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127)
    Record416Pad(
        128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143,
        144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159,
        160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174, 175,
        176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191)
    Record416Pad(
        192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207,
        208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223,
        224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239,
        240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255)
    Record416Rows = {
        {a = Record416Catalog.a, enabled = true, disabled = false, cleared = nil,
         padding = 3, freshtrue = true, freshfalse = false, freshnil = nil},
        {a = Record416Catalog.a, enabled = false, disabled = true, cleared = nil,
         padding = 3, freshtrue = false, freshfalse = true, freshnil = nil},
    }
end
Record416Rows = "old"
Record416Build()
for i, row in ipairs(Record416Rows) do
    assert(row.a == i and row.enabled ~= row.disabled)
    assert(row.cleared == nil and row.padding == 3 and row.freshnil == nil)
    assert(row.freshtrue == row.enabled and row.freshfalse == row.disabled)
    local count = 0
    for _ in pairs(row) do count = count + 1 end
    assert(count == 6)
    print(i, row.a, row.enabled, row.disabled, count)
end
assert(Record416Rows[1].enabled == true and Record416Rows[2].enabled == false)
for fail = 1, 2 do
    reads = 0
    Record416Fail = fail
    Record416Rows = "old"
    local ok, err = pcall(Record416Build)
    assert(not ok and err == "primitive-stop" and Record416Rows == "old")
    print(fail, reads)
end
