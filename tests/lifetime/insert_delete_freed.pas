program InsertDeleteFreed;

// The results of `+`, Concat, Insert and Delete own their values and are freed (no leak; the
// runner checks the heap after the finishing stage): strings and arrays of strings in a local, a
// global, a record field, an array element and a `var` parameter, including a source that is
// the target itself.

type
  TBox = record
    Name: string;
    Items: TArray<string>;
  end;

var
  G: TArray<string>;
  GS: string;

procedure Edit(var S: string; var A: TArray<string>);
begin
  Insert('<', S, 1);
  Delete(S, 2, 1);
  Insert(S, A, 0);
  Delete(A, 1, 1);
end;

procedure Run;
var
  S: string;
  A, B: TArray<string>;
  Box: TBox = Default(TBox);
  Boxes: TArray<TBox>;
begin
  S := 'ab' + Copy('cde', 2, 1);
  Insert(S, S, 2);
  Delete(S, 1, 2);
  A := ['x' + S, 'y' + S];
  B := A + A;
  B := Concat(B, ['z' + S], A);
  Insert(B, B, 1);
  Insert('w' + S, B, 0);
  Delete(B, 2, 3);
  Box.Name := 'box' + S;
  Box.Items := B;
  Insert('n', Box.Name, 1);
  Insert('i' + S, Box.Items, 0);
  Delete(Box.Items, 0, 2);
  SetLength(Boxes, 1);
  Boxes[0] := Box;
  Insert('m', Boxes[0].Name, 2);
  Delete(Boxes[0].Items, 0, 1);
  Edit(S, A);
  Edit(Box.Name, Box.Items);
  WriteLn(S, ' ', Length(A), ' ', Length(B), ' ', Box.Name, ' ', Length(Box.Items), ' ',
    Boxes[0].Name, ' ', Length(Boxes[0].Items));
end;

begin
  Run;
  GS := 'g' + 'h';
  G := [GS, GS + GS];
  Insert(GS + '!', G, 1);
  Delete(G, 0, 1);
  G := G + G;
  Insert('?', GS, 1);
  WriteLn(GS, ' ', Length(G), ' ', G[0]);
end.
