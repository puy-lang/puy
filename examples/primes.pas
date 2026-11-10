program Primes;

// The sieve of Eratosthenes over a dynamic array.

const
  Limit = 100;

var
  IsComposite: TArray<Boolean>;
  Found: TArray<Integer>;
  I, J: Integer;

begin
  SetLength(IsComposite, Limit + 1);
  for I := 2 to Limit do
    if not IsComposite[I] then
    begin
      Found := Found + [I];
      J := I * I;
      while J <= Limit do
      begin
        IsComposite[J] := True;
        Inc(J, I);
      end;
    end;

  WriteLn('Primes up to ', Limit, ': ', Length(Found));
  for var P in Found do
    Write(P:4);
  WriteLn;
end.
