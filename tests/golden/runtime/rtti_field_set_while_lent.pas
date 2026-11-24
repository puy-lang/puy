program RttiFieldSetWhileLent;

// Writing a field that holds no counted value through System.Rtti while a record field is lent
// (a `var` parameter) is not refused: only a counted value can hold a lent location
// (rtti.md 5.6). The integers written run over the numbers of the heap blocks.

uses System.SysUtils, System.Rtti;

type
  TRec = record
    A, B: Integer;
    S: string;
  end;

  TFoo = class
  public
    N: Integer;
  end;

var
  Foo: TFoo;
  G: TRec;
  Recs: array[0..3] of TRec;

procedure Probe(var X: Integer);
begin
  var F := TRttiContext.Create.GetType(TFoo).GetField('N');
  var Refused := 0;
  for var I := 1 to 2000 do
    try
      F.SetValue(Foo, TValue.From<Integer>(I));
    except
      on E: Exception do
        Inc(Refused);
    end;
  WriteLn(Foo.N, ' refused ', Refused);
end;

begin
  Foo := TFoo.Create;
  G.S := 'x';
  Probe(G.A);
  Recs[2].S := 'y';
  Probe(Recs[2].A);
  Foo.Free;
end.
