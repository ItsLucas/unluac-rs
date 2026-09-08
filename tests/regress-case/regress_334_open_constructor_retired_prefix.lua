-- Fixed call results stay rooted until their immediate overwrite, including nil tails.
local events = {}
local weak = setmetatable({}, { __mode = "v" })
local function read(index)
    events[#events + 1] = index
    collectgarbage("collect")
    if index == 3 then
        assert(weak[1] and weak[2])
        return 31, nil, 37
    end
    local value = { index }
    weak[index] = value
    return value
end
local function build(producer)
    local values = { producer(1), producer(2), producer(3) }
    local replacement = { 0, 0, 0 }
    for index = 1, 3 do
        replacement[index] = values[index]
    end
    print("regress_334", replacement[1][1], replacement[2][1],
        replacement[3], values[4], values[5])
end
build(read)
print("regress_334_order", table.concat(events, ","))
