local function run(mode, enabled, found, remaining)
  local trace = {}
  local function note(value)
    trace[#trace + 1] = value
  end
  local function clock()
    note("clock")
    return 1
  end
  repeat
    if mode == 3 then
      if enabled then
        note("prepare")
        local limit = 10
        if found then
          local value = remaining
          value = value - clock()
          if not (value < limit) then
            break
          end
        end
        note("shared")
      end
    end
  until true
  if mode == 2 or mode == 3 then
    note("after")
  end
  return table.concat(trace, ",")
end

for mode = 1, 3 do
  for remaining = 10, 12 do
    print(run(mode, false, false, remaining))
    print(run(mode, false, true, remaining))
    print(run(mode, true, false, remaining))
    print(run(mode, true, true, remaining))
  end
end
