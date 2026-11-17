program AnonErrors;

// Errors of anonymous methods (objects.md 9.6, 9.10, 16.2): E0751 for what cannot be captured (a
// `var` and an `out` parameter, an open array parameter, the `Result` of the enclosing function,
// `Self` of a record method), E0752 for an anonymous method going to a procedural type or a
// method reference type, E0729 where no type is expected, E0702 for parameters that do not match,
// E0707 for writing a `const` parameter or the enclosing routine's loop variable, E0712 for
// `Break` outside a loop of its own, and E3200 for a class implementing an anonymous method type;
// E0713 for the initial value of a global variable (a constant expression).

type
  TIntFunc = reference to function(N: Integer): Integer;
  TPlain = procedure;
  TEvent = procedure of object;

  TRec = record
    X: Integer;
    function Getter: TProc;
  end;

  TRunner = class(TRefObject, TProc)
  end;

var
  Global: TProc = procedure begin end;

function TRec.Getter: TProc;
begin
  Result := procedure
    begin
      WriteLn(X);
    end;
end;

procedure Params(var V: Integer; out O: Integer; const A: array of Integer; const C: Integer);
var
  P: TProc;
begin
  O := 0;
  P := procedure
    begin
      WriteLn(V);
      O := 1;
      WriteLn(Length(A));
      C := 2;
    end;
  P();
end;

function Res: Integer;
var
  P: TProc;
begin
  Result := 0;
  P := procedure
    begin
      Result := 1;
    end;
  P();
end;

procedure Kinds;
var
  Plain: TPlain;
  Event: TEvent;
  F: TIntFunc;
  I: Integer;
begin
  Plain := procedure begin end;
  Event := procedure begin end;
  F := function(S: string): Integer begin Result := 0 end;
  F := procedure(N: Integer) begin end;
  var Q := procedure begin end;
  for I := 1 to 2 do
  begin
    F := function(N: Integer): Integer
      begin
        I := N;
        Result := N;
        Break;
      end;
  end;
end;

begin
end.
