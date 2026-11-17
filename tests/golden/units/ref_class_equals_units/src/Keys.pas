unit Keys;

// A reference counted class declaring its own `Equal`, whose generated `Equals` is checked
// against a descendant declared in the program (equality.md 3.3, 8.3).

interface

type
  TKey = class(TRefObject)
    V: Integer;
    constructor Create(AV: Integer);
    class operator Equal(const A, B: TKey): Boolean;
    function GetHashCode: Int64; override;
  end;

implementation

constructor TKey.Create(AV: Integer);
begin
  V := AV;
end;

class operator TKey.Equal(const A, B: TKey): Boolean;
begin
  Result := A.V = B.V;
end;

function TKey.GetHashCode: Int64;
begin
  Result := V;
end;

end.
