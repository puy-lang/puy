unit Impl;

interface

function ImplKind: Integer;

implementation

uses Kinds;

function ImplKind: Integer;
begin
  Result := Ord(kOne);
end;

end.
