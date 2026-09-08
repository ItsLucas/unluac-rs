local function accept(trace)
    trace[#trace + 1] = "yes"
end

local function reject(trace)
    trace[#trace + 1] = "no"
end

local function run(bits, variant)
    local trace = {}
    local function probe(mask, label)
        trace[#trace + 1] = label
        if math.floor(bits / mask) % 2 == 1 then
            return 0
        end
        return false
    end
    if variant == 1 then
        if probe(1, "a") or ((probe(2, "b") or probe(4, "c")) and probe(8, "d")) then
            accept(trace)
        else
            reject(trace)
        end
    elseif variant == 2 then
        if probe(1, "a") or ((probe(2, "b") or not probe(4, "c")) and probe(8, "d")) then
            accept(trace)
        else
            reject(trace)
        end
    elseif variant == 3 then
        if probe(1, "a") and ((probe(2, "b") and probe(4, "c")) or probe(8, "d")) then
            accept(trace)
        else
            reject(trace)
        end
    else
        if probe(1, "a") and ((probe(2, "b") and not probe(4, "c")) or probe(8, "d")) then
            accept(trace)
        else
            reject(trace)
        end
    end
    print(variant, bits, table.concat(trace, ":"))
end

for variant = 1, 4 do
    for bits = 0, 15 do
        run(bits, variant)
    end
end
