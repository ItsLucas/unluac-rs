-- The final CALL expands, including zero results, holes and trailing nils.
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
Open414Values = {}
for i = 1, 105 do
    if i % 3 ~= 0 then Open414Values[i] = i end
end
function Open414Make(which)
    trace[#trace + 1] = which
    if Open414Fail == which then error("open-stop", 0) end
    if which == 1 then return "fixed", "discarded" end
    return unpack(Open414Values, 1, Open414Count)
end
function Open414Build()
    local tag = 17
    local rows = {
        -- region414 fixed begin
        Open414Make(1),
        -- region414 fixed end
        Open414Make(2)
    }
    function Open414Read() return tag, rows end
end
for count = 0, 105 do
    Open414Count = count
    Open414Build()
    local tag, rows = Open414Read()
    assert(tag == 17 and rows[1] == "fixed")
    local present = 0
    for i = 1, count do
        assert(rows[1 + i] == Open414Values[i])
        if rows[1 + i] ~= nil then present = present + 1 end
    end
    assert(rows[count + 2] == nil)
    print(count, #rows, present)
end
local previous = Open414Read
for fail = 1, 2 do
    Open414Fail = fail
    local ok, err = pcall(Open414Build)
    assert(not ok and err == "open-stop" and Open414Read == previous)
end
print(table.concat(trace, ","))
