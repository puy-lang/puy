// Script mode: no `program` header is needed for a small script.
for var I := 1 to 20 do
  if I mod 15 = 0 then
    WriteLn('FizzBuzz')
  else if I mod 3 = 0 then
    WriteLn('Fizz')
  else if I mod 5 = 0 then
    WriteLn('Buzz')
  else
    WriteLn(I);
