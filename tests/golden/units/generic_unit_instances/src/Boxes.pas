unit Boxes;

// A generic type declared in a unit's interface section: the program instantiates it, and the
// unit instantiates it too; one key is one type wherever it is written (generics.md 4.1).

interface

type
  TBox<T> = class
  private
    FItem: T;
  public
    constructor Create(const AItem: T);
    function Get: T;
  end;

function IntBox(N: Integer): TBox<Integer>;

implementation

constructor TBox<T>.Create(const AItem: T);
begin
  FItem := AItem;
end;

function TBox<T>.Get: T;
begin
  Result := FItem;
end;

function IntBox(N: Integer): TBox<Integer>;
begin
  Result := TBox<Integer>.Create(N);
end;

end.
