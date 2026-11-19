unit Impl;

interface

function ImplKind: Integer;

implementation

uses Kinds;

function ImplKind: Integer;
begin
  Result := Ord(TKind.kOne);
end;

end.
