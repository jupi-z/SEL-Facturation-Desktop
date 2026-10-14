"""PDF vectoriel A4 sélectionnable (ReportLab) pour les décomptes SEL."""
from __future__ import annotations
from io import BytesIO
import json
from decimal import Decimal
from pathlib import Path
from xml.sax.saxutils import escape
import sys

from reportlab.lib import colors
from reportlab.lib.enums import TA_CENTER, TA_LEFT, TA_RIGHT
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle
from reportlab.lib.units import mm
from reportlab.platypus import (SimpleDocTemplate, Paragraph, Spacer, Table,
                                TableStyle, Image, KeepTogether)
from domain import fmt

BLUE=colors.HexColor('#084479')
GREEN=colors.HexColor('#268D5C')
PALE=colors.HexColor('#E8F3FA')
INK=colors.HexColor('#1A2A3A')
GREY=colors.HexColor('#566575')
BORDER=colors.HexColor('#D8E2E9')


def p(value):
    return escape(str(value if value not in (None,'') else 'Non renseigné'))


def period_fr(period):
    names=['janvier','février','mars','avril','mai','juin','juillet','août','septembre','octobre','novembre','décembre']
    try:
        year,month=period.split('-');return names[int(month)-1].capitalize()+' '+year
    except (ValueError,IndexError):return period


def render_invoice(invoice, payments) -> bytes:
    inv=dict(invoice)
    client=json.loads(inv['client_snapshot']);co=json.loads(inv['company_snapshot']);d=json.loads(inv['calc_snapshot'])
    paid=sum((Decimal(str(x['amount'])) for x in payments),Decimal('0'))
    balance=Decimal(d['total'])-paid
    out=BytesIO()
    doc=SimpleDocTemplate(out,pagesize=A4,rightMargin=15*mm,leftMargin=15*mm,topMargin=16*mm,bottomMargin=17*mm,
                          title=f"Décompte {inv['number']} - SEL",author='SEL Facturation')
    normal=ParagraphStyle('normal',fontName='Helvetica',fontSize=8.6,leading=13,textColor=INK,spaceAfter=0)
    muted=ParagraphStyle('muted',parent=normal,fontSize=8,textColor=GREY)
    small=ParagraphStyle('small',parent=normal,fontSize=7.5,leading=11)
    section=ParagraphStyle('section',fontName='Helvetica-Bold',fontSize=10.3,leading=14,textColor=BLUE,spaceBefore=7,spaceAfter=5)
    title=ParagraphStyle('title',fontName='Helvetica-Bold',fontSize=17,leading=22,textColor=BLUE,spaceAfter=4)
    twhite=ParagraphStyle('twhite',fontName='Helvetica-Bold',fontSize=8.5,leading=12,textColor=colors.white)
    right=ParagraphStyle('right',parent=normal,alignment=TA_RIGHT)
    val=ParagraphStyle('val',parent=normal,alignment=TA_RIGHT,fontSize=9)
    story=[]

    # identite SEL + titre de document
    logo=Path(getattr(sys,'_MEIPASS',Path(__file__).resolve().parent))/'static'/'sel-logo.png'
    mark=Image(str(logo),width=31*mm,height=30*mm) if logo.exists() else Paragraph('<b>SEL</b>',title)
    company_cells=[mark,
        [Paragraph('<b>'+p(co.get('company_name','SOCIÉTÉ D’ÉNERGIE DU LUALABA'))+'</b>',ParagraphStyle('co',parent=normal,fontName='Helvetica-Bold',fontSize=12.4,leading=15,textColor=BLUE)),
         Paragraph(p(co.get('tagline','')),muted),
         Paragraph(p(co.get('address','')),small),
         Paragraph(' · '.join([p(v) for v in (co.get('phone'),co.get('email'),co.get('website')) if v]),small)],
        [Paragraph('DÉCOMPTE<br/>D’ÉLECTRICITÉ',title),
         Paragraph('N° '+p(inv['number']),ParagraphStyle('ref',parent=normal,fontName='Helvetica-Bold',fontSize=9,alignment=TA_RIGHT,textColor=BLUE)),
         Paragraph('Émis le '+p(inv['issue_date']),right)]]
    table=Table([company_cells],colWidths=[32*mm,80*mm,66*mm])
    table.setStyle(TableStyle([('VALIGN',(0,0),(-1,-1),'TOP'),('LEFTPADDING',(0,0),(-1,-1),0),('RIGHTPADDING',(0,0),(-1,-1),4),('BOTTOMPADDING',(0,0),(-1,-1),9),('LINEBELOW',(0,0),(-1,-1),2,BLUE)]))
    story.extend([table,Spacer(1,7)])
    warning=Table([[Paragraph('<b>DOCUMENT NON AUTHENTIFIÉ PAR LA DGI</b> — les références et le QR fiscal officiels sont absents.',small)]],colWidths=[178*mm])
    warning.setStyle(TableStyle([('BACKGROUND',(0,0),(-1,-1),colors.HexColor('#FFF3DC')),('BOX',(0,0),(-1,-1),.6,colors.HexColor('#E5B660')),('LEFTPADDING',(0,0),(-1,-1),10),('TOPPADDING',(0,0),(-1,-1),8),('BOTTOMPADDING',(0,0),(-1,-1),8)]))
    story.extend([warning,Spacer(1,10)])

    info_left='<b>INFORMATIONS CLIENT</b><br/>' + '<br/>'.join([
         f"<b>Nom :</b> {p(client.get('name'))}",f"<b>Code :</b> {p(client.get('code'))}",
         f"<b>Contrat :</b> {p(client.get('contract_number'))}",f"<b>Adresse :</b> {p(client.get('address'))}",
         f"<b>Catégorie :</b> {p(client.get('category'))}",f"<b>NIF client :</b> {p(client.get('nif'))}"])
    info_right='<b>INFORMATIONS COMPTEUR</b><br/>'+'<br/>'.join([
         f"<b>Centre :</b> {p(client.get('center'))}",f"<b>Point de service :</b> {p(client.get('service_point'))}",
         f"<b>N° compteur :</b> {p(client.get('meter_number'))}",f"<b>Coefficient :</b> {p(d.get('coefficient'))}",
         f"<b>Période :</b> {p(period_fr(inv['billing_period']))}",f"<b>Échéance :</b> {p(inv['due_date'])}"])
    details=Table([[Paragraph(info_left,normal),Paragraph(info_right,normal)]],colWidths=[89*mm,89*mm])
    details.setStyle(TableStyle([('BACKGROUND',(0,0),(-1,-1),colors.HexColor('#F5F9FB')),('BOX',(0,0),(-1,-1),.6,BORDER),('INNERGRID',(0,0),(-1,-1),.4,BORDER),('VALIGN',(0,0),(-1,-1),'TOP'),('TOPPADDING',(0,0),(-1,-1),11),('BOTTOMPADDING',(0,0),(-1,-1),11),('LEFTPADDING',(0,0),(-1,-1),11)]))
    story.extend([details,Paragraph('RELEVÉ ET CONSOMMATION',section)])
    usage=Table([[Paragraph('<b>Index précédent</b>',small),Paragraph('<b>Index actuel</b>',small),Paragraph('<b>Consommation</b>',small),Paragraph('<b>Période facturée</b>',small)],
                 [p(d['previous']),p(d['current']),p(d['kwh'])+' kWh',p(period_fr(inv['billing_period']))]],colWidths=[43*mm,43*mm,43*mm,49*mm])
    usage.setStyle(TableStyle([('BACKGROUND',(0,0),(-1,0),PALE),('TEXTCOLOR',(0,0),(-1,0),BLUE),('LINEBELOW',(0,0),(-1,0),.8,BLUE),('INNERGRID',(0,0),(-1,-1),.45,BORDER),('BOX',(0,0),(-1,-1),.65,BORDER),('ALIGN',(0,0),(-1,-1),'CENTER'),('VALIGN',(0,0),(-1,-1),'MIDDLE'),('TOPPADDING',(0,0),(-1,-1),8),('BOTTOMPADDING',(0,0),(-1,-1),8)]))
    story.extend([usage,Paragraph('DÉTAIL DE LA CONSOMMATION FACTURÉE',section)])
    lines=[[Paragraph('<b>Tranche tarifaire</b>',small),Paragraph('<b>Quantité (kWh)</b>',small),Paragraph('<b>Prix unitaire</b>',small),Paragraph('<b>Montant</b>',small)]]
    for row in d['lines']:
        lines.append([p(row['range']),p(row['quantity']),fmt(row['unit_price']),fmt(row['amount'])])
    if not d['lines']:lines.append(['Aucune consommation','0','—','0,00'])
    table=Table(lines,colWidths=[49*mm,44*mm,43*mm,42*mm],repeatRows=1)
    table.setStyle(TableStyle([('BACKGROUND',(0,0),(-1,0),PALE),('TEXTCOLOR',(0,0),(-1,0),BLUE),('INNERGRID',(0,0),(-1,-1),.4,BORDER),('BOX',(0,0),(-1,-1),.6,BORDER),('VALIGN',(0,0),(-1,-1),'MIDDLE'),('ALIGN',(1,1),(-1,-1),'RIGHT'),('LEFTPADDING',(0,0),(-1,-1),8),('RIGHTPADDING',(0,0),(-1,-1),8),('TOPPADDING',(0,0),(-1,-1),7),('BOTTOMPADDING',(0,0),(-1,-1),7)]))
    story.append(table)
    recap=[('Sous-total énergie',fmt(d['energy'])),(f"Taxe locale ({p(d['local_rate'])} %)",fmt(d['local_tax'])),
           ('Frais supplémentaires',fmt(d['fee'])),(f"TVA ({p(d['vat_rate'])} %) — base {fmt(d['vat_basis'])}",fmt(d['vat'])),
           ('MONTANT TOTAL ('+p(inv['currency'])+')',fmt(d['total']))]
    rec=Table([[Paragraph(a,normal),Paragraph(b,right)] for a,b in recap],colWidths=[130*mm,48*mm])
    rec.setStyle(TableStyle([('LINEABOVE',(0,-1),(-1,-1),1.2,BLUE),('BACKGROUND',(0,-1),(-1,-1),PALE),('TEXTCOLOR',(0,-1),(-1,-1),BLUE),('TOPPADDING',(0,0),(-1,-1),6),('BOTTOMPADDING',(0,0),(-1,-1),6),('RIGHTPADDING',(1,0),(1,-1),8),('LEFTPADDING',(0,0),(0,-1),8)]))
    story.extend([Spacer(1,7),rec,Spacer(1,10)])
    payment_summary=Paragraph(f"<b>Paiements enregistrés :</b> {fmt(paid)} {p(inv['currency'])} &nbsp;&nbsp; <b>Solde :</b> {fmt(balance)} {p(inv['currency'])}",normal)
    story.append(payment_summary)
    if inv['status']=='VOID':story.append(Paragraph('<font color="red"><b>DOCUMENT ANNULÉ - NE PAS UTILISER</b></font>',section))
    if inv['note']:story.extend([Spacer(1,6),Paragraph('<b>Note :</b> '+p(inv['note']),small)])
    story.extend([Spacer(1,10),Paragraph('<b>MENTIONS ENTREPRISE :</b> RCCM : '+p(co.get('rccm'))+' &nbsp;&nbsp; NIF : '+p(co.get('nif'))+' &nbsp;&nbsp; ID NAT : '+p(co.get('id_nat')),small),
                  Paragraph('Les taxes et tarifs repris sur ce document doivent avoir été validés par SEL. Aucun code ni QR DGI n’a été généré.',muted)])
    def footer(canvas, document):
        canvas.saveState()
        w,h=A4
        canvas.setStrokeColor(BORDER);canvas.line(15*mm,14*mm,w-15*mm,14*mm)
        canvas.setFont('Helvetica',7);canvas.setFillColor(GREY)
        canvas.drawString(15*mm,10*mm,'SEL Facturation | Document local de gestion — non fiscalisé')
        canvas.drawRightString(w-15*mm,10*mm,f'Page {document.page}')
        canvas.restoreState()
    doc.build(story,onFirstPage=footer,onLaterPages=footer)
    return out.getvalue()
