-- Enclosing branch joins cannot become a loop-body fallthrough continuation.
-- unluac: expect-not-contains [[goto]]
-- unluac: expect-not-contains [[unluac error]]
local function probe(name, value)
    io.write(name)
    return value
end

local function run(enabled, limit)
    local score, total, accepted = 0, 0, false
    if enabled then
        for index = 1, limit do
            if index > 1 then
                score = index
                total = total + index
                if probe("a", index == 2) or probe("b", index == 4) then
                    accepted = true
                end
            end
            io.write("tail", index, ";")
            if index == 4 then
                break
            end
        end
        if accepted then
            score = 0
        end
    end
    return score, total, accepted
end

for limit = 0, 5 do
    print("regress_340", limit, run(true, limit))
end
print("regress_340", run(false, 5))

-- Both outcomes of the nested guard complete the same enclosing branch.
local function state(value, kind)
    probe("read", true)
    return value, kind, nil, false
end

local function common_transfer(enabled, forced, value, kind)
    local flag = enabled
    if probe("force", forced) or probe("other", false) then
        flag = true
    end
    if flag then
        probe("first", true)
        local a, b, c, d = state(value, kind)
        if a ~= 0 then
            probe("first-extra", true)
        end
    else
        probe("second", true)
        local a, b, c, d = state(value, kind)
        if a ~= 0 and (b == 1 or b == 2 or b == 3) then
            probe("second-extra", true)
        end
    end
    probe("tail", true)
end

for bits = 0, 15 do
    common_transfer(bits % 2 == 1, bits % 4 >= 2, math.floor(bits / 4) % 2,
        math.floor(bits / 8))
    print("regress_340_transfer", bits)
end

-- Completing a nested guard must not execute the unrelated sequence tail.
local function completion_boundary(a, b, c)
    local x = 0
    if (a and b) or c then
        if a and b then return x end
        if b then x = x + 2 else x = x + 7 end
    else
        if (a and b) or c then
            if b then x = x + 7 else x = x + 8 end
        else
            if not b then return x end
            x = x + 9
        end
    end
    return x
end

-- A terminal arm does not execute the continuation transfer from its sibling.
local function terminal_sibling(a, b, c)
    local x = 0
    if a and (b or c) then
        if (a and b) or c then
            if c then x = x + 1 else x = x + 5 end
        else
            if not b then return x end
            x = x + 6
        end
    else
        if c then return x end
        if a and (b or c) then x = x + 3 else x = x + 2 end
    end
    if a and b then return x end
    if c then x = x + 8 else x = x + 6 end
    return x
end

local function terminal_successor(a, b, c)
    local x = 0
    if x > 10 then
        if (a and b) or c then return x end
        if (a and b) or c then return x end
        x = x + 4
    else
        if x > 10 then
            if a then x = x + 4 else x = x + 2 end
        else
            if c then x = x + 1 else x = x + 5 end
        end
    end
    if c then
        if c then x = x + 9 else x = x + 8 end
    else
        if (a and b) or c then x = x + 9 else x = x + 3 end
    end
    return x
end

for bits = 0, 7 do
    local a, b, c = bits % 2 == 1, bits % 4 >= 2, bits >= 4
    print("regress_340_completion", bits, completion_boundary(a, b, c),
        terminal_sibling(a, b, c), terminal_successor(a, b, c))
end
