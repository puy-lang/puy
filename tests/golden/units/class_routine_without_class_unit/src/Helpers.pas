unit Helpers;

interface

procedure Run;

implementation

class procedure Bar;
begin
end;

class function Answer: Integer;
begin
  Result := 42;
end;

procedure Run;
begin
  WriteLn('Run');
end;

end.
