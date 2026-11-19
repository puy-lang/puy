unit Marks;

interface

type
  MarkAttribute = class(TCustomAttribute)
  public
    constructor Create(N: Integer);
  end;

  TBase = class
  public
    function GetN: Integer;
  end;

implementation

constructor MarkAttribute.Create(N: Integer);
begin
end;

function TBase.GetN: Integer;
begin
  Result := 7;
end;

end.
