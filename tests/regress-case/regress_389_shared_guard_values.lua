local function choose(trace, a, b, c, d)
    local function probe(label, value)
        trace[#trace + 1] = label
        return value
    end
    return probe("a", a) or ((probe("b", b) or probe("c", c)) and probe("d", d))
end

for bits = 0, 15 do
    local trace = {}
    local a = bits % 2 == 1 and 0 or false
    local b = math.floor(bits / 2) % 2 == 1 and "b" or false
    local c = math.floor(bits / 4) % 2 == 1 and "" or nil
    local d = math.floor(bits / 8) % 2 == 1 and "d" or false
    local result = choose(trace, a, b, c, d)
    print(bits, type(result), tostring(result), table.concat(trace, ":"))
end
