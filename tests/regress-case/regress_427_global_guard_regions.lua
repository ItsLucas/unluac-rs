-- 条件调用结果与分支内闭包复用同一临时槽；false路径也须保持后继帧。
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = "v"})
function Guard427Load(value) return {}, {} end
function Guard427Row(index) return {index = index} end
function Guard427Gate()
    trace[#trace + 1] = "gate"
    if Guard427Fail then error("guard-call-stop", 0) end
    if Guard427Decision == "object" then
        local result = {}
        weak.condition = result
        return result, "discarded"
    end
    return Guard427Decision, "discarded"
end
function Guard427Build()
    Guard427Load("prefix")
    local tag = 17
    local rows = {{Guard427Row(1), 2}, {Guard427Row(2), 4}}
    if Guard427Gate() then
        function Guard427Installed(value) return tag, rows, value end
    end
end
setfenv(Guard427Build, setmetatable({}, {
    __index = function(_, key)
        collectgarbage("collect")
        if key ~= "Guard427Load" then
            trace[#trace + 1] = "get:" .. key .. ":" .. (weak.condition and "live" or "dead")
        end
        return _G[key]
    end,
    __newindex = function(_, key, value)
        collectgarbage("collect")
        trace[#trace + 1] = "store:" .. key .. ":" .. (weak.condition and "live" or "dead")
        if Guard427StoreFail then error("guard-store-stop", 0) end
        _G[key] = value
    end,
}))
for choice = 0, 4 do
    if choice == 0 then Guard427Decision = nil
    elseif choice == 1 then Guard427Decision = false
    elseif choice == 2 then Guard427Decision = true
    elseif choice == 3 then Guard427Decision = 0
    else Guard427Decision = "object" end
    trace = {}
    weak = setmetatable({}, {__mode = "v"})
    Guard427Installed = "old"
    Guard427Build()
    if choice <= 1 then assert(Guard427Installed == "old")
    else
        local tag, rows, value = Guard427Installed(19)
        assert(tag == 17 and value == 19 and rows[1][1].index == 1 and rows[2][2] == 4)
    end
    print(choice, table.concat(trace, "|"))
end
for fail = 1, 2 do
    trace = {}
    weak = setmetatable({}, {__mode = "v"})
    Guard427Installed = "old"
    Guard427Decision = "object"
    Guard427Fail = fail == 1
    Guard427StoreFail = fail == 2
    local ok, err = pcall(Guard427Build)
    assert(not ok and Guard427Installed == "old")
    assert(err == (fail == 1 and "guard-call-stop" or "guard-store-stop"))
    print(fail, table.concat(trace, "|"))
end
