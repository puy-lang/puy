unit Shared;

interface

const
  Limit = 10;

function IntToStr(N: Integer): string;

implementation

function IntToStr(N: Integer): string;
begin
  Result := 'shared';
end;

end.
