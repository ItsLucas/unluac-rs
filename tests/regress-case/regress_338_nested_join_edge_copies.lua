-- A nested short-circuit edge carries three values to its enclosing join.
-- unluac: expect-not-contains [[goto]]
-- unluac: expect-not-contains [[unluac error]]
local function probe(name, value)
    io.write(name)
    return value
end

local function compute(enabled, kind, first, second, initial)
    local score, total = 0, 0
    if enabled then
        local accepted = initial
        if kind > 0 then
            total = kind * 3
            if kind == 1 then
                score = 1
                if probe("a", first) or probe("b", second) then
                    accepted = true
                end
            end
        end
        if accepted then
            score = 0
        end
    end
    return score, total
end

for kind = 0, 2 do
    for bits = 0, 7 do
        print("regress_338", kind, bits,
            compute(true, kind, bits % 2 == 1, bits % 4 >= 2, bits >= 4))
    end
end
print("regress_338", compute(false, 1, true, true, true))

-- A predicate's callee lookup must remain ahead of its argument and call.
local function dispatch_with_lookup(a, b, value, object)
    if (probe("a", a) or probe("b", b)) and object.gate(value) == 0 then
        if a then
            probe("then-a", true)
        elseif b then
            probe("then-b", true)
        end
        probe("then-tail", true)
        if value == 0 then
            probe("then-last", true)
        end
    else
        if a and b then
            probe("else-ab", true)
        end
        probe("else-tail", true)
    end
    probe("tail", true)
end

local object = setmetatable({}, {
    __index = function(_, key)
        probe("lookup-" .. key, true)
        return function(value)
            return probe("call", value)
        end
    end
})

for bits = 0, 7 do
    dispatch_with_lookup(bits % 2 == 1, bits % 4 >= 2, math.floor(bits / 4), object)
    print("regress_338_lookup", bits)
end

local function argument_order(value)
    if object.gate(probe("arg", value)) == 0 then
        probe("zero", true)
    end
end

local function skipped_callee(enabled, value)
    local callback = object.gate
    if enabled and callback(value) == 0 then
        probe("selected", true)
    end
end

argument_order(0)
argument_order(1)
skipped_callee(false, 0)
skipped_callee(true, 0)
skipped_callee(true, 1)
print("regress_338_order")

-- A call-valued predicate argument keeps outer lookup, inner lookup and open-pack order.
local function nested_dispatch(enabled, mode, receiver)
    if receiver.first(receiver.read()) or enabled or receiver.last(receiver.read()) then
        if mode then
            probe("on", true)
        else
            probe("off", true)
        end
    else
        probe("off", true)
    end
end

local function nested_predicate(first, last, enabled, mode)
    local receiver = setmetatable({}, {
        __index = function(_, key)
            probe("nested-lookup-" .. key, true)
            return function(...)
                probe("nested-call-" .. key, true)
                if key == "read" then
                    return 3, 4
                end
                print("regress_338_nested_args", ...)
                if key == "first" then
                    return first
                end
                return last
            end
        end
    })
    nested_dispatch(enabled, mode, receiver)
end

for bits = 0, 15 do
    nested_predicate(bits % 2 == 1, bits % 4 >= 2, bits % 8 >= 4, bits >= 8)
    print("regress_338_nested", bits)
end
