local function accept(label, reads, calls)
    print(label, "yes", reads, calls)
end

local function reject(label, reads, calls)
    print(label, "no", reads, calls)
end

local function run(label, readings, enabled)
    local reads, calls = 0, 0
    local object = setmetatable({}, {
        __index = function()
            reads = reads + 1
            return readings[reads]
        end,
    })
    local function active()
        calls = calls + 1
        return enabled
    end
    if object.code == 11 or ((object.code == 17 or object.code == 23) and active())
        or object.code == 31 then
        accept(label, reads, calls)
    else
        reject(label, reads, calls)
    end
end

run("first", { 11 }, true)
run("left", { 0, 17 }, true)
run("right", { 0, 0, 23 }, true)
run("inactive", { 0, 17, 31 }, false)
run("last", { 0, 0, 0, 31 }, true)
run("none", { 0, 0, 0, 0 }, false)
