program Nested;

uses
  System.SysUtils;

// Nested routines: an inner routine reads and writes the variables and
// parameters of the routine around it.

type
  TWords = TArray<string>;
  TUsed = array[1..16] of Boolean;

function Permutations(const Letters: string): TWords;
var
  Used: TUsed;
  Current: string;

  procedure Extend;
  begin
    if Length(Current) = Length(Letters) then
    begin
      Result := Result + [Current];
      Exit;
    end;
    for var I := 1 to Length(Letters) do
      if not Used[I] then
      begin
        Used[I] := True;
        Current := Current + Letters[I];
        Extend;
        SetLength(Current, Length(Current) - 1);
        Used[I] := False;
      end;
  end;

begin
  Result := [];
  Used := Default(TUsed);
  Current := '';
  Extend;
end;

function Summary(const Values: array of Integer): string;
var
  Total: Integer;

  procedure Add(const Name: string; Value: Integer);
  begin
    if Result <> '' then
      Result := Result + ', ';
    Result := Result + Name + '=' + IntToStr(Value);
  end;

begin
  Result := '';
  Total := 0;
  for var V in Values do
    Total := Total + V;
  Add('count', Length(Values));
  Add('total', Total);
  if Length(Values) > 0 then
    Add('mean', Total div Length(Values));
end;

begin
  for var W in Permutations('abc') do
    Write(W, ' ');
  WriteLn;
  WriteLn(Length(Permutations('abcd')), ' permutations of abcd');
  WriteLn(Summary([3, 1, 4, 1, 5, 9, 2, 6]));
end.
