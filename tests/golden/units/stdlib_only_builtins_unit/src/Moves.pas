unit Moves;

interface

uses System.Generics.Collections;

procedure MoveFirst(var A: TArray<Integer>);

implementation

procedure MoveFirst(var A: TArray<Integer>);
begin
  ArrayMoveRange(A, 0, 1, 1);
end;

end.
