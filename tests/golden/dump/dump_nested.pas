program DumpNested;

uses System.SysUtils;

// The dumps of nested routines (semantics.md 4.9, 9; bytecode.md 2.x, 5, 5.1): the `outer`
// variables, the static link (`.reg link`, `outer=`) and `OuterRef`. `Middle` uses no variable of
// `Run` but takes the link to pass it on to `Deep`; `Twice` uses nothing and takes no link. `S` is
// used by a nested routine, so it is copied, not lent, to the `const` parameter of `Show`.

procedure Show(const T: string);
begin
  WriteLn(T);
end;

procedure Run(N: Integer);
var
  S: string;

  function Twice(K: Integer): Integer;
  begin
    Result := K * 2;
  end;

  procedure Middle;

    procedure Deep;
    begin
      S := S + IntToStr(Twice(N));
    end;

  begin
    Deep;
  end;

begin
  S := 'n=';
  Middle;
  Show(S);
end;

begin
  Run(21);
end.
