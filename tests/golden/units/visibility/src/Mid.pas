unit Mid;

interface

const
  Early = BaseValue;

function Total: Integer;

implementation

uses Base;

const
  Hidden = 2;

function Total: Integer;
begin
  Result := BaseValue + Hidden;
end;

end.
